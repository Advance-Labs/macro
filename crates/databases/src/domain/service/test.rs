//! Service tests: the domain service over in-memory fakes for every port, so
//! every write path (schema changes, typed ops, lifecycle) runs without
//! Postgres, and every allow/deny decision is asserted at the service
//! boundary.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use entity_access::domain::models::{
    AccessLevel, Entity, EntityAccessReceipt, EntityPermission, EntityType, RequiredPermission,
};
use macro_event_broker::{EventBrokerError, MacroEvent, MacroEventBroker};
use macro_user_id::user_id::MacroUserIdStr;
use models_databases::{CellValue, CellWrite, DatabaseOp, OpResult, OptionRef, RowChanges};
use models_properties::service::property_definition::PropertyDefinition;
use models_properties::service::property_definition_with_options::PropertyDefinitionWithOptions;
use models_properties::service::property_option::{PropertyOption, PropertyOptionValue};
use models_properties::service::property_value::PropertyValue;
use models_properties::shared::{DataType, EntityType as PropertyEntityType, PropertyOwner};
use uuid::Uuid;

use super::*;
use crate::domain::models::{
    CardPosition, Column, ColumnBinding, ColumnConfig, DatabaseView, NewOption, OpRefusal,
    PropertyDefinitionId, RowId, RowRef, TableDeletion, TableOrderOutcome, TableVersion, ViewId,
    Write, Writes, WritesOutcome,
};

mod casts;
mod columns;
mod delete_table;
mod discovery;
mod infer_column_type;
mod ops;
mod options;
mod relations;
mod rename_column;
mod saved_queries;
mod sharing;
mod tables;
mod views;

const OWNER: &str = "macro|owner@macro.com";
const VIEWER: &str = "macro|viewer@macro.com";
const STRANGER: &str = "macro|stranger@macro.com";

fn user(id: &'static str) -> MacroUserIdStr<'static> {
    MacroUserIdStr::parse_from_str(id).expect("valid user id")
}

fn viewer(id: &'static str) -> Viewer {
    Viewer {
        user_id: user(id),
        acting_bot: None,
    }
}

#[derive(Debug, thiserror::Error)]
#[error("fake failure")]
struct FakeError;

/// Shared mutable world the fakes read and write.
#[derive(Default)]
struct World {
    databases: Vec<Database>,
    tables: Vec<Table>,
    columns: Vec<Column>,
    definitions: HashMap<PropertyDefinitionId, PropertyDefinitionWithOptions>,
    /// Row identities per table, in position order.
    rows: HashMap<TableId, Vec<RowRef>>,
    /// The cell store: a row's cells keyed by definition.
    cells: HashMap<RowId, HashMap<Uuid, PropertyValue>>,
    grants: HashMap<String, Vec<(DatabaseId, AccessLevel)>>,
    published: Vec<(TableId, TableVersion)>,
    /// Every awareness relay the service asked for.
    awareness: Vec<(DatabaseId, String, Awareness)>,
    share_updates: Vec<Vec<models_permissions::share_permission::channel_share_permission::UpdateChannelSharePermission>>,
    /// Every `macro.databases` envelope the service handed the broker.
    broker_events: Vec<serde_json::Value>,
    /// Every `settle_inference` call, newest last.
    settled: Vec<(TableId, Vec<PropertyDefinitionId>)>,
    /// Simulate a parent removed between domain validation and the write.
    table_write_not_found: bool,
    /// Saved queries, oldest first.
    queries: Vec<SavedQuery>,
    /// How many write batches the cell store was handed.
    write_batches: usize,
    /// The shared definitions each user may change, as the properties system
    /// answers it.
    editable_definitions: HashMap<String, Vec<PropertyDefinitionId>>,
    /// Every table's views.
    views: Vec<DatabaseView>,
    /// Where each board's cards sit.
    positions: HashMap<ViewId, Vec<CardPosition>>,
}

/// Store views a schema change rewrote; `false` when one is gone.
fn rewrite_views(w: &mut World, views: &[DatabaseView]) -> bool {
    for view in views {
        let Some(stored) = w.views.iter_mut().find(|stored| stored.id == view.id) else {
            return false;
        };
        *stored = view.clone();
    }
    true
}

type Shared = Arc<Mutex<World>>;

#[derive(Clone)]
struct FakeRepo(Shared);
#[derive(Clone)]
struct FakeDefs(Shared);
#[derive(Clone)]
struct FakeCells(Shared);
#[derive(Clone)]
struct FakeEvents(Shared);
#[derive(Clone)]
struct FakeAccess(Shared);

impl DatabasesRepo for FakeRepo {
    type Error = FakeError;
    async fn create_database(
        &self,
        cmd: &CreateDatabase,
        starter_table_name: &str,
    ) -> Result<Database, FakeError> {
        let database = Database {
            id: Uuid::new_v4(),
            name: cmd.name.clone(),
            owner_id: cmd.owner_id.as_ref().to_string(),
            created_at: Utc::now(),
            trashed_at: None,
        };
        let mut w = self.0.lock().unwrap();
        w.databases.push(database.clone());
        let position = format!("{:04}", w.tables.len());
        w.tables.push(Table {
            id: Uuid::new_v4(),
            database_id: database.id,
            name: starter_table_name.to_string(),
            position,
            version: TableVersion(0),
        });
        w.grants
            .entry(cmd.owner_id.as_ref().to_string())
            .or_default()
            .push((database.id, AccessLevel::Owner));
        Ok(database)
    }
    async fn get_database(
        &self,
        id: DatabaseId,
    ) -> Result<Option<(Database, Vec<Table>)>, FakeError> {
        let w = self.0.lock().unwrap();
        Ok(w.databases.iter().find(|d| d.id == id).map(|d| {
            (
                d.clone(),
                w.tables
                    .iter()
                    .filter(|t| t.database_id == id)
                    .cloned()
                    .collect(),
            )
        }))
    }
    async fn rename_database(&self, id: DatabaseId, name: &str) -> Result<(), FakeError> {
        let mut w = self.0.lock().unwrap();
        if let Some(database) = w.databases.iter_mut().find(|d| d.id == id) {
            database.name = name.to_string();
        }
        Ok(())
    }
    async fn trash_database(
        &self,
        id: DatabaseId,
        trashed_at: chrono::DateTime<Utc>,
    ) -> Result<(), FakeError> {
        let mut w = self.0.lock().unwrap();
        if let Some(database) = w.databases.iter_mut().find(|d| d.id == id) {
            database.trashed_at = Some(trashed_at);
        }
        Ok(())
    }
    async fn restore_database(&self, id: DatabaseId) -> Result<(), FakeError> {
        let mut w = self.0.lock().unwrap();
        if let Some(database) = w.databases.iter_mut().find(|d| d.id == id) {
            database.trashed_at = None;
        }
        Ok(())
    }
    async fn delete_database(&self, id: DatabaseId) -> Result<(), FakeError> {
        let mut w = self.0.lock().unwrap();
        w.databases.retain(|d| d.id != id);
        let table_ids: Vec<TableId> = w
            .tables
            .iter()
            .filter(|t| t.database_id == id)
            .map(|t| t.id)
            .collect();
        w.tables.retain(|t| t.database_id != id);
        w.columns.retain(|c| !table_ids.contains(&c.table_id));
        let row_ids: Vec<RowId> = table_ids
            .iter()
            .filter_map(|table_id| w.rows.remove(table_id))
            .flatten()
            .map(|row| row.id)
            .collect();
        w.cells.retain(|row_id, _| !row_ids.contains(row_id));
        // The Postgres adapter purges `entity_access` rows in the same
        // transaction; the fake's grant map stands in for that table.
        for grants in w.grants.values_mut() {
            grants.retain(|(database_id, _)| *database_id != id);
        }
        Ok(())
    }
    async fn create_table(&self, cmd: &CreateTable) -> Result<TableMutationOutcome, FakeError> {
        let mut w = self.0.lock().unwrap();
        if w.table_write_not_found {
            return Ok(TableMutationOutcome::NotFound);
        }
        if w.tables
            .iter()
            .any(|table| table.database_id == cmd.database_id && same_name(&table.name, &cmd.name))
        {
            return Ok(TableMutationOutcome::Conflict);
        }
        let table = Table {
            id: Uuid::new_v4(),
            database_id: cmd.database_id,
            name: cmd.name.clone(),
            position: format!("{:04}", w.tables.len()),
            version: TableVersion(0),
        };
        w.tables.push(table.clone());
        Ok(TableMutationOutcome::Applied(table))
    }
    async fn rename_table(
        &self,
        table: &Table,
        name: &str,
        previous_name: &str,
    ) -> Result<TableMutationOutcome, FakeError> {
        let mut world = self.0.lock().unwrap();
        if world.table_write_not_found {
            return Ok(TableMutationOutcome::NotFound);
        }
        let Some(current) = world
            .tables
            .iter_mut()
            .find(|candidate| candidate.id == table.id && candidate.name == previous_name)
        else {
            return Ok(TableMutationOutcome::Conflict);
        };
        current.name = name.to_string();
        current.version.0 += 1;
        Ok(TableMutationOutcome::Applied(current.clone()))
    }
    async fn reorder_tables(
        &self,
        database_id: DatabaseId,
        ids: &[TableId],
    ) -> Result<TableOrderOutcome, FakeError> {
        let mut world = self.0.lock().unwrap();
        if world.table_write_not_found {
            return Ok(TableOrderOutcome::NotFound);
        }
        let mut current: Vec<TableId> = world
            .tables
            .iter()
            .filter(|table| table.database_id == database_id)
            .map(|table| table.id)
            .collect();
        let mut requested = ids.to_vec();
        current.sort();
        requested.sort();
        if current != requested {
            return Ok(TableOrderOutcome::Conflict);
        }
        for (index, id) in ids.iter().enumerate() {
            let table = world
                .tables
                .iter_mut()
                .find(|table| table.id == *id)
                .unwrap();
            table.position = format!("{:04}", index + 1);
            table.version.0 += 1;
        }
        world.tables.sort_by(|a, b| a.position.cmp(&b.position));
        Ok(TableOrderOutcome::Applied(
            ids.iter()
                .map(|id| {
                    world
                        .tables
                        .iter()
                        .find(|table| table.id == *id)
                        .unwrap()
                        .clone()
                })
                .collect(),
        ))
    }
    async fn delete_table(&self, table: &Table) -> Result<TableDeletion, FakeError> {
        let mut w = self.0.lock().unwrap();
        if w.table_write_not_found
            || !w
                .databases
                .iter()
                .any(|d| d.id == table.database_id && d.trashed_at.is_none())
            || !w.tables.iter().any(|t| t.id == table.id)
        {
            return Ok(TableDeletion::NotFound);
        }
        if w.tables
            .iter()
            .filter(|t| t.database_id == table.database_id)
            .count()
            <= 1
        {
            return Ok(TableDeletion::LastTable);
        }
        w.tables.retain(|t| t.id != table.id);
        w.columns.retain(|c| c.table_id != table.id);
        // The schema's cleanup triggers take the rows' cells with them.
        for row in w.rows.remove(&table.id).unwrap_or_default() {
            w.cells.remove(&row.id);
        }
        Ok(TableDeletion::Deleted)
    }
    async fn create_column(
        &self,
        table_id: TableId,
        property_definition_id: PropertyDefinitionId,
        cmd: &CreateColumn,
    ) -> Result<(ColumnId, TableVersion), FakeError> {
        let mut w = self.0.lock().unwrap();
        let column = Column {
            infer_type: cmd.infer_type,
            display_name: None,
            id: Uuid::new_v4(),
            table_id,
            property_definition_id,
            position: format!("{:04}", w.columns.len()),
            config: cmd.config.clone(),
        };
        w.columns.push(column.clone());
        // Unlike Postgres the fake leaves the version alone, so the seeded
        // tests' versions count only data writes.
        let version = w
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .ok_or(FakeError)?
            .version;
        Ok((column.id, version))
    }
    async fn rename_column(
        &self,
        table: &Table,
        column: &Column,
        name: &str,
    ) -> Result<Option<RenameColumnOutcome>, FakeError> {
        let mut world = self.0.lock().unwrap();
        let Some(table_index) = world
            .tables
            .iter()
            .position(|current| current.id == table.id && current.version == table.version)
        else {
            return Ok(None);
        };
        let Some(column_index) = world.columns.iter().position(|current| {
            current.id == column.id
                && current.table_id == table.id
                && current.display_name == column.display_name
        }) else {
            return Ok(None);
        };
        world.tables[table_index].version.0 += 1;
        world.columns[column_index].display_name = Some(name.to_string());
        Ok(Some(RenameColumnOutcome {
            column: world.columns[column_index].clone(),
            table_version: world.tables[table_index].version,
        }))
    }
    async fn infer_column_type(
        &self,
        table: &Table,
        column: &Column,
        definition_id: PropertyDefinitionId,
    ) -> Result<Option<TableVersion>, FakeError> {
        let mut w = self.0.lock().unwrap();
        let has_value = w.rows.get(&table.id).is_some_and(|rows| {
            rows.iter().any(|row| {
                w.cells
                    .get(&row.id)
                    .is_some_and(|cells| cells.contains_key(&column.property_definition_id))
            })
        });
        if has_value {
            return Ok(None);
        }
        let Some(t) = w
            .tables
            .iter()
            .position(|t| t.id == table.id && t.version == table.version)
        else {
            return Ok(None);
        };
        let Some(c) = w.columns.iter().position(|c| {
            c.id == column.id
                && c.table_id == table.id
                && c.property_definition_id == column.property_definition_id
                && c.infer_type
        }) else {
            return Ok(None);
        };
        w.columns[c].property_definition_id = definition_id;
        w.columns[c].infer_type = false;
        w.tables[t].version.0 += 1;
        Ok(Some(w.tables[t].version))
    }
    async fn delete_column(
        &self,
        table: &Table,
        column: &Column,
        views: &[DatabaseView],
    ) -> Result<Option<ColumnSchemaOutcome>, FakeError> {
        let mut w = self.0.lock().unwrap();
        let Some(t) = w
            .tables
            .iter()
            .position(|t| t.id == table.id && t.version == table.version)
        else {
            return Ok(None);
        };
        let Some(c) = w
            .columns
            .iter()
            .position(|c| c.id == column.id && c.table_id == table.id)
        else {
            return Ok(None);
        };
        if !rewrite_views(&mut w, views) {
            return Ok(None);
        }
        let removed = w.columns.remove(c);
        let rows: Vec<RowId> = w
            .rows
            .get(&table.id)
            .map(|rows| rows.iter().map(|row| row.id).collect())
            .unwrap_or_default();
        for row in rows {
            if let Some(cells) = w.cells.get_mut(&row) {
                cells.remove(&removed.property_definition_id);
            }
        }
        w.tables[t].version.0 += 1;
        Ok(Some(ColumnSchemaOutcome {
            table_versions: HashMap::from([(table.id, w.tables[t].version)]),
        }))
    }
    async fn reorder_columns(
        &self,
        table: &Table,
        ids: &[ColumnId],
    ) -> Result<Option<TableVersion>, FakeError> {
        let mut w = self.0.lock().unwrap();
        let Some(t) = w
            .tables
            .iter()
            .position(|t| t.id == table.id && t.version == table.version)
        else {
            return Ok(None);
        };
        for (index, id) in ids.iter().enumerate() {
            let Some(c) = w
                .columns
                .iter_mut()
                .find(|c| c.id == *id && c.table_id == table.id)
            else {
                return Ok(None);
            };
            c.position = format!("{:04}", index + 1);
        }
        w.tables[t].version.0 += 1;
        Ok(Some(w.tables[t].version))
    }
    async fn row_refs(&self, table_id: TableId) -> Result<Vec<RowRef>, FakeError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .rows
            .get(&table_id)
            .cloned()
            .unwrap_or_default())
    }
    async fn table_versions(
        &self,
        table_ids: &[TableId],
    ) -> Result<HashMap<TableId, TableVersion>, FakeError> {
        let w = self.0.lock().unwrap();
        Ok(w.tables
            .iter()
            .filter(|t| table_ids.contains(&t.id))
            .map(|t| (t.id, t.version))
            .collect())
    }
    async fn databases_by_ids(&self, ids: &[DatabaseId]) -> Result<Vec<Database>, FakeError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .databases
            .iter()
            .filter(|d| ids.contains(&d.id))
            .cloned()
            .collect())
    }
    async fn tables_for_databases(
        &self,
        database_ids: &[DatabaseId],
    ) -> Result<Vec<Table>, FakeError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .tables
            .iter()
            .filter(|t| database_ids.contains(&t.database_id))
            .cloned()
            .collect())
    }
    async fn columns_for_tables(&self, table_ids: &[TableId]) -> Result<Vec<Column>, FakeError> {
        let mut columns: Vec<Column> = self
            .0
            .lock()
            .unwrap()
            .columns
            .iter()
            .filter(|c| table_ids.contains(&c.table_id))
            .cloned()
            .collect();
        columns.sort_by(|a, b| (a.table_id, &a.position).cmp(&(b.table_id, &b.position)));
        Ok(columns)
    }
    async fn views_for_tables(
        &self,
        table_ids: &[TableId],
    ) -> Result<Vec<DatabaseView>, FakeError> {
        let mut views: Vec<DatabaseView> = self
            .0
            .lock()
            .unwrap()
            .views
            .iter()
            .filter(|view| table_ids.contains(&view.table_id))
            .cloned()
            .collect();
        views.sort_by(|a, b| (a.table_id, &a.position).cmp(&(b.table_id, &b.position)));
        Ok(views)
    }
    async fn view_positions(&self, view_id: ViewId) -> Result<Vec<CardPosition>, FakeError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .positions
            .get(&view_id)
            .cloned()
            .unwrap_or_default())
    }
    async fn save_query(
        &self,
        database_id: Option<DatabaseId>,
        definition: &QueryDefinition,
        created_by: &MacroUserIdStr<'_>,
    ) -> Result<SavedQuery, FakeError> {
        let saved = SavedQuery {
            id: Uuid::now_v7(),
            definition: definition.clone(),
            database_id,
            created_by: Some(created_by.as_ref().to_string()),
            created_at: Utc::now(),
        };
        self.0.lock().unwrap().queries.push(saved.clone());
        Ok(saved)
    }
    async fn get_query(&self, id: QueryId) -> Result<Option<SavedQuery>, FakeError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .queries
            .iter()
            .find(|query| query.id == id)
            .cloned())
    }
}

impl CellStore for FakeCells {
    type Error = FakeError;
    async fn cells(
        &self,
        rows: &[RowId],
    ) -> Result<HashMap<RowId, HashMap<PropertyDefinitionId, PropertyValue>>, FakeError> {
        let w = self.0.lock().unwrap();
        Ok(rows
            .iter()
            .filter_map(|row| w.cells.get(row).map(|cells| (*row, cells.clone())))
            .collect())
    }
    async fn column_cells(
        &self,
        rows: &[RowId],
        definition: PropertyDefinitionId,
    ) -> Result<HashMap<RowId, PropertyValue>, FakeError> {
        let w = self.0.lock().unwrap();
        Ok(rows
            .iter()
            .filter_map(|row| {
                let value = w.cells.get(row)?.get(&definition)?;
                Some((*row, value.clone()))
            })
            .collect())
    }
    async fn replace_column(
        &self,
        table: &Table,
        replacement: &ColumnReplacement,
        views: &[DatabaseView],
    ) -> Result<Option<TableVersion>, FakeError> {
        let mut w = self.0.lock().unwrap();
        let Some(t) = w
            .tables
            .iter()
            .position(|t| t.id == table.id && t.version == table.version)
        else {
            return Ok(None);
        };
        let Some(c) = w.columns.iter().position(|c| {
            c.id == replacement.column.id
                && c.table_id == table.id
                && c.property_definition_id == replacement.column.property_definition_id
        }) else {
            return Ok(None);
        };
        if !rewrite_views(&mut w, views) {
            return Ok(None);
        }
        for (row, value) in &replacement.values {
            w.cells
                .entry(*row)
                .or_default()
                .insert(replacement.definition_id, value.clone());
        }
        w.columns[c].property_definition_id = replacement.definition_id;
        w.columns[c].config = replacement.config.clone();
        w.columns[c].infer_type = false;
        w.tables[t].version.0 += 1;
        Ok(Some(w.tables[t].version))
    }
    async fn add_options(
        &self,
        table_id: TableId,
        options: &[NewOption],
    ) -> Result<Option<TableVersion>, FakeError> {
        let mut world = self.0.lock().unwrap();
        let Some(table_index) = world.tables.iter().position(|table| table.id == table_id) else {
            return Ok(None);
        };
        for option in options {
            let definition = world
                .definitions
                .get_mut(&option.definition_id)
                .ok_or(FakeError)?;
            let display_order = definition
                .property_options
                .iter()
                .map(|existing| existing.display_order)
                .max()
                .map_or(0, |highest| highest + 1);
            definition.property_options.push(PropertyOption {
                id: option.id,
                property_definition_id: option.definition_id,
                display_order,
                value: option.value.clone(),
                color: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            });
        }
        world.tables[table_index].version.0 += 1;
        Ok(Some(world.tables[table_index].version))
    }
    async fn apply_writes(&self, writes: &Writes) -> Result<WritesOutcome, FakeError> {
        let mut w = self.0.lock().unwrap();
        w.write_batches += 1;
        let before = (
            w.tables.clone(),
            w.columns.clone(),
            w.definitions.clone(),
            w.rows.clone(),
            w.cells.clone(),
            w.settled.clone(),
            w.views.clone(),
            w.positions.clone(),
        );
        let outcome = apply_in_world(&mut w, writes);
        if !matches!(outcome, WritesOutcome::Applied { .. }) {
            (
                w.tables,
                w.columns,
                w.definitions,
                w.rows,
                w.cells,
                w.settled,
                w.views,
                w.positions,
            ) = before;
        }
        Ok(outcome)
    }
}

/// A first value settles the columns it landed in, as the cell store does in
/// its transaction.
fn settle(w: &mut World, table_id: TableId, definitions: Vec<PropertyDefinitionId>) {
    if definitions.is_empty() {
        return;
    }
    for column in &mut w.columns {
        if column.table_id == table_id && definitions.contains(&column.property_definition_id) {
            column.infer_type = false;
        }
    }
    w.settled.push((table_id, definitions));
}

/// The fake cell store's batch, applied straight to the world; the caller
/// rolls the world back unless everything applied.
fn apply_in_world(w: &mut World, writes: &Writes) -> WritesOutcome {
    for table_id in writes
        .writes
        .iter()
        .flat_map(|write| write.versioned_tables().iter().copied())
    {
        let live = w.tables.iter().find(|t| t.id == table_id).is_some_and(|t| {
            w.databases
                .iter()
                .any(|d| d.id == t.database_id && d.trashed_at.is_none())
        });
        if !live || w.table_write_not_found {
            return WritesOutcome::TableNotFound(table_id);
        }
    }
    for option in &writes.options {
        let Some(definition) = w.definitions.get_mut(&option.definition_id) else {
            return WritesOutcome::TableNotFound(Uuid::nil());
        };
        let display_order = definition.property_options.len() as i32;
        definition.property_options.push(PropertyOption {
            id: option.id,
            property_definition_id: option.definition_id,
            display_order,
            value: option.value.clone(),
            color: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        });
    }
    let mut inserted = Vec::new();
    for (index, write) in writes.writes.iter().enumerate() {
        match write {
            Write::InsertRows { table_id, rows } => {
                let mut ids = Vec::new();
                for cells in rows {
                    let id = Uuid::now_v7();
                    let table_rows = w.rows.entry(*table_id).or_default();
                    table_rows.push(RowRef {
                        id,
                        position: format!("{:04}", table_rows.len()),
                    });
                    ids.push(id);
                    if !cells.is_empty() {
                        w.cells.entry(id).or_default().extend(cells.iter().cloned());
                    }
                    let valued: Vec<_> = cells.iter().map(|(definition, _)| *definition).collect();
                    settle(w, *table_id, valued);
                }
                inserted.push(ids);
            }
            Write::UpdateRows { table_id, rows } => {
                for (row, cells) in rows {
                    let owned = w
                        .rows
                        .get(table_id)
                        .is_some_and(|rows| rows.iter().any(|r| r.id == *row));
                    if !owned {
                        return WritesOutcome::MissingRow {
                            write: index,
                            row: *row,
                        };
                    }
                    let stored = w.cells.entry(*row).or_default();
                    for (definition, value) in cells {
                        match value {
                            Some(value) => {
                                stored.insert(*definition, value.clone());
                            }
                            None => {
                                stored.remove(definition);
                            }
                        }
                    }
                    let valued: Vec<_> = cells
                        .iter()
                        .filter(|(_, value)| value.is_some())
                        .map(|(definition, _)| *definition)
                        .collect();
                    settle(w, *table_id, valued);
                }
                inserted.push(Vec::new());
            }
            Write::DeleteRows { table_id, rows } => {
                for row in rows {
                    let table_rows = w.rows.entry(*table_id).or_default();
                    let Some(position) = table_rows.iter().position(|r| r.id == *row) else {
                        return WritesOutcome::MissingRow {
                            write: index,
                            row: *row,
                        };
                    };
                    table_rows.remove(position);
                    w.cells.remove(row);
                }
                inserted.push(Vec::new());
            }
            Write::UpdateOption {
                definition_id,
                option_id,
                value,
                color,
                ..
            } => {
                let Some(option) = w.definitions.get_mut(definition_id).and_then(|definition| {
                    definition
                        .property_options
                        .iter_mut()
                        .find(|option| option.id == *option_id)
                }) else {
                    return WritesOutcome::MissingOption { write: index };
                };
                if let Some(value) = value {
                    option.value = value.clone();
                }
                if let Some(color) = color {
                    option.color = color.clone();
                }
                inserted.push(Vec::new());
            }
            Write::DeleteOption {
                tables,
                definition_id,
                option_id,
                views,
                ..
            } => {
                if !rewrite_views(w, views) {
                    return WritesOutcome::MissingView { write: index };
                }
                let boards: Vec<ViewId> = w
                    .views
                    .iter()
                    .filter(|view| tables.contains(&view.table_id))
                    .map(|view| view.id)
                    .collect();
                for board in boards {
                    if let Some(placed) = w.positions.get_mut(&board) {
                        placed.retain(|card| card.lane != Some(*option_id));
                    }
                }
                let Some(definition) = w.definitions.get_mut(definition_id) else {
                    return WritesOutcome::MissingOption { write: index };
                };
                let before = definition.property_options.len();
                definition
                    .property_options
                    .retain(|option| option.id != *option_id);
                if definition.property_options.len() == before {
                    return WritesOutcome::MissingOption { write: index };
                }
                for cells in w.cells.values_mut() {
                    if let Some(PropertyValue::SelectOption(options)) = cells.get_mut(definition_id)
                    {
                        options.retain(|option| option != option_id);
                        if options.is_empty() {
                            cells.remove(definition_id);
                        }
                    }
                }
                inserted.push(Vec::new());
            }
            Write::CreateView { view } => {
                if w.views.iter().any(|other| {
                    other.table_id == view.table_id && other.name.eq_ignore_ascii_case(&view.name)
                }) {
                    return WritesOutcome::ViewNameTaken { write: index };
                }
                w.views.push(view.clone());
                inserted.push(Vec::new());
            }
            Write::UpdateView { view, regrouped } => {
                if !rewrite_views(w, std::slice::from_ref(view)) {
                    return WritesOutcome::MissingView { write: index };
                }
                if *regrouped {
                    w.positions.remove(&view.id);
                }
                inserted.push(Vec::new());
            }
            Write::DeleteView { table_id, view_id } => {
                let before = w.views.len();
                w.views
                    .retain(|view| !(view.id == *view_id && view.table_id == *table_id));
                if w.views.len() == before {
                    return WritesOutcome::MissingView { write: index };
                }
                w.positions.remove(view_id);
                inserted.push(Vec::new());
            }
            Write::OrderViews {
                table_id,
                positions,
            } => {
                for placed in positions {
                    let Some(view) = w
                        .views
                        .iter_mut()
                        .find(|view| view.id == placed.view && view.table_id == *table_id)
                    else {
                        return WritesOutcome::MissingView { write: index };
                    };
                    view.position = placed.position.clone();
                }
                inserted.push(Vec::new());
            }
            Write::MoveCard {
                table_id,
                view_id,
                row,
                positions,
                cell: (definition, value),
            } => {
                let owned = w
                    .rows
                    .get(table_id)
                    .is_some_and(|rows| rows.iter().any(|r| r.id == *row));
                if !owned {
                    return WritesOutcome::MissingRow {
                        write: index,
                        row: *row,
                    };
                }
                let stored = w.cells.entry(*row).or_default();
                match value {
                    Some(value) => {
                        stored.insert(*definition, value.clone());
                    }
                    None => {
                        stored.remove(definition);
                    }
                }
                let placed = w.positions.entry(*view_id).or_default();
                for position in positions {
                    placed.retain(|card| card.row != position.row);
                    placed.push(position.clone());
                }
                inserted.push(Vec::new());
            }
        }
    }
    for (table_id, row) in &writes.related_rows {
        if !w
            .rows
            .get(table_id)
            .is_some_and(|rows| rows.iter().any(|r| r.id == *row))
        {
            return WritesOutcome::MissingRelatedRow(*row);
        }
    }
    let mut table_versions = HashMap::new();
    for write in writes.writes.iter().filter(|write| write.changes()) {
        for table_id in write.versioned_tables() {
            if table_versions.contains_key(table_id) {
                continue;
            }
            let table = w.tables.iter_mut().find(|t| t.id == *table_id).unwrap();
            table.version.0 += 1;
            table_versions.insert(*table_id, table.version);
        }
    }
    WritesOutcome::Applied {
        inserted,
        table_versions,
    }
}

impl ColumnDefinitionStore for FakeDefs {
    type Error = FakeError;
    async fn resolve_binding(
        &self,
        database_id: DatabaseId,
        _viewer: &Viewer,
        binding: &ColumnBinding,
    ) -> Result<Option<PropertyDefinitionId>, FakeError> {
        match binding {
            ColumnBinding::ExistingDefinition(id) => Ok(self
                .0
                .lock()
                .unwrap()
                .definitions
                .contains_key(id)
                .then_some(*id)),
            ColumnBinding::NewDefinition {
                name,
                data_type,
                is_multi_select,
                // Options are attached through `add_options`, as in Postgres.
                options: _,
            } => {
                let def = definition(
                    name,
                    *data_type,
                    *is_multi_select,
                    PropertyOwner::Database { database_id },
                );
                let id = def.definition.id;
                self.0.lock().unwrap().definitions.insert(id, def);
                Ok(Some(id))
            }
        }
    }
    async fn create_typed_definition(
        &self,
        database_id: DatabaseId,
        name: &str,
        data_type: DataType,
        is_multi_select: bool,
        specific_entity_type: Option<PropertyEntityType>,
    ) -> Result<PropertyDefinitionWithOptions, FakeError> {
        let mut def = definition(
            name,
            data_type,
            is_multi_select,
            PropertyOwner::Database { database_id },
        );
        def.definition.specific_entity_type = specific_entity_type;
        self.0
            .lock()
            .unwrap()
            .definitions
            .insert(def.definition.id, def.clone());
        Ok(def)
    }
    async fn delete_unused_definition(&self, id: PropertyDefinitionId) -> Result<(), FakeError> {
        self.0.lock().unwrap().definitions.remove(&id);
        Ok(())
    }
    async fn add_options(
        &self,
        definition_id: PropertyDefinitionId,
        values: &[PropertyOptionValue],
    ) -> Result<Vec<PropertyOption>, FakeError> {
        let mut w = self.0.lock().unwrap();
        let def = w.definitions.get_mut(&definition_id).ok_or(FakeError)?;
        let mut display_order = def
            .property_options
            .iter()
            .map(|o| o.display_order)
            .max()
            .map_or(0, |highest| highest + 1);
        for value in values {
            def.property_options.push(PropertyOption {
                id: Uuid::new_v4(),
                property_definition_id: definition_id,
                display_order,
                value: value.clone(),
                color: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            });
            display_order += 1;
        }
        Ok(def.property_options.clone())
    }
    async fn definitions(
        &self,
        ids: &[PropertyDefinitionId],
    ) -> Result<Vec<PropertyDefinitionWithOptions>, FakeError> {
        let w = self.0.lock().unwrap();
        Ok(ids
            .iter()
            .filter_map(|id| w.definitions.get(id).cloned())
            .collect())
    }
    async fn editable_definitions(
        &self,
        viewer: &Viewer,
        ids: &[PropertyDefinitionId],
    ) -> Result<Vec<PropertyDefinitionId>, FakeError> {
        let w = self.0.lock().unwrap();
        let editable = w
            .editable_definitions
            .get(viewer.user_id.as_ref())
            .cloned()
            .unwrap_or_default();
        Ok(ids
            .iter()
            .copied()
            .filter(|id| editable.contains(id))
            .collect())
    }
}

impl TableEventPublisher for FakeEvents {
    type Error = FakeError;
    async fn table_changed(
        &self,
        _database_id: DatabaseId,
        table_id: TableId,
        version: TableVersion,
    ) -> Result<(), FakeError> {
        self.0.lock().unwrap().published.push((table_id, version));
        Ok(())
    }
    async fn awareness(
        &self,
        database_id: DatabaseId,
        user_id: &MacroUserIdStr<'_>,
        state: &Awareness,
    ) -> Result<(), FakeError> {
        self.0.lock().unwrap().awareness.push((
            database_id,
            user_id.as_ref().to_string(),
            state.clone(),
        ));
        Ok(())
    }
}

impl AccessDirectory for FakeAccess {
    type Error = FakeError;
    async fn accessible_databases(
        &self,
        viewer: &Viewer,
    ) -> Result<Vec<(DatabaseId, AccessLevel)>, FakeError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .grants
            .get(viewer.user_id.as_ref())
            .cloned()
            .unwrap_or_default())
    }
    async fn database_access(
        &self,
        viewer: &Viewer,
        database_id: DatabaseId,
    ) -> Result<Option<AccessLevel>, FakeError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .grants
            .get(viewer.user_id.as_ref())
            .into_iter()
            .flatten()
            .filter(|(id, _)| *id == database_id)
            .map(|(_, level)| *level)
            .max())
    }
}

#[derive(Clone)]
struct RecordingBroker(Shared);

impl MacroEventBroker for RecordingBroker {
    fn send_event<E: MacroEvent + ?Sized>(
        &self,
        event: &E,
    ) -> Result<tokio::task::JoinHandle<Result<(), EventBrokerError>>, EventBrokerError> {
        let envelope =
            serde_json::to_value(event.event()).map_err(EventBrokerError::Serialization)?;
        self.0.lock().unwrap().broker_events.push(envelope);
        Ok(tokio::spawn(async { Ok(()) }))
    }
}

type Service =
    DatabasesServiceImpl<FakeRepo, FakeDefs, FakeCells, FakeEvents, FakeAccess, RecordingBroker>;

fn service(world: &Shared) -> Service {
    DatabasesServiceImpl::new(
        FakeRepo(world.clone()),
        FakeDefs(world.clone()),
        FakeCells(world.clone()),
        FakeEvents(world.clone()),
        FakeAccess(world.clone()),
        RecordingBroker(world.clone()),
    )
}

fn definition(
    name: &str,
    data_type: DataType,
    multi: bool,
    owner: PropertyOwner,
) -> PropertyDefinitionWithOptions {
    PropertyDefinitionWithOptions {
        definition: PropertyDefinition {
            id: Uuid::new_v4(),
            owner,
            display_name: name.into(),
            data_type,
            is_multi_select: multi,
            specific_entity_type: (data_type == DataType::Entity)
                .then_some(PropertyEntityType::User),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            is_system: false,
            is_metadata: false,
        },
        property_options: vec![],
    }
}

fn receipt<T: RequiredPermission>(
    database_id: DatabaseId,
    user: &'static str,
    level: AccessLevel,
) -> EntityAccessReceipt<T> {
    EntityAccessReceipt::try_new_authenticated_user(
        self::user(user),
        Entity {
            entity_id: database_id.to_string(),
            entity_type: EntityType::Database,
        },
        EntityPermission::AccessLevel {
            access_level: level,
        },
    )
    .expect("level satisfies the requirement")
}

fn edit(database_id: DatabaseId) -> EntityAccessReceipt<EditAccessLevel> {
    receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner)
}

fn table_version(world: &Shared, table_id: TableId) -> TableVersion {
    world
        .lock()
        .unwrap()
        .tables
        .iter()
        .find(|table| table.id == table_id)
        .unwrap()
        .version
}

fn option_id(world: &Shared, definition_id: PropertyDefinitionId, label: &str) -> Uuid {
    world.lock().unwrap().definitions[&definition_id]
        .property_options
        .iter()
        .find(|option| option.value == PropertyOptionValue::String(label.into()))
        .unwrap()
        .id
}

fn row_ids(world: &Shared, table_id: TableId) -> Vec<RowId> {
    world.lock().unwrap().rows[&table_id]
        .iter()
        .map(|row| row.id)
        .collect()
}

fn cell(world: &Shared, row: RowId, definition_id: PropertyDefinitionId) -> Option<PropertyValue> {
    world
        .lock()
        .unwrap()
        .cells
        .get(&row)
        .and_then(|cells| cells.get(&definition_id))
        .cloned()
}

/// Insert one row per name into the seeded Guests table, answering their ids.
async fn insert_names(seeded: &Seeded, names: &[&str]) -> Vec<RowId> {
    let results = seeded
        .service
        .apply_ops(
            edit(seeded.database_id),
            viewer(OWNER),
            vec![DatabaseOp::InsertRows {
                table: seeded.table_id,
                rows: names
                    .iter()
                    .map(|name| {
                        vec![CellWrite {
                            column: seeded.name_column.id,
                            value: CellValue::Text((*name).into()),
                        }]
                    })
                    .collect(),
                create_missing_options: false,
            }],
        )
        .await
        .unwrap();
    let [OpResult::RowsWritten { inserted, .. }] = results.as_slice() else {
        panic!("expected one insert, got {results:?}");
    };
    inserted.clone()
}

/// The seeded world: database `Offsite` with one table
/// `Guests(Name TEXT, Status SELECT[Going|Declined], Plus ones NUMBER)`
/// holding one row (`Sam`, `Going`, `2`), owned by OWNER and shared
/// View-only with VIEWER.
struct Seeded {
    world: Shared,
    service: Service,
    database_id: DatabaseId,
    table_id: TableId,
    /// Sam's row.
    row_id: RowId,
    name_column: Column,
    status_column: Column,
    plus_ones_column: Column,
}

async fn seeded() -> Seeded {
    let world: Shared = Arc::default();
    let service = self::service(&world);
    let database = service
        .create_database(CreateDatabase {
            name: "Offsite".into(),
            owner_id: user(OWNER),
            acting_bot: None,
        })
        .await
        .unwrap();
    let table_id = {
        let mut w = world.lock().unwrap();
        w.tables[0].name = "Guests".into();
        w.tables[0].id
    };
    let name_column = service
        .create_column(
            receipt::<EditAccessLevel>(database.id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "Name".into(),
                    data_type: DataType::String,
                    is_multi_select: false,
                    options: vec![],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    let status_column = service
        .create_column(
            receipt::<EditAccessLevel>(database.id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "Status".into(),
                    data_type: DataType::SelectString,
                    is_multi_select: false,
                    options: vec!["Going".into(), "Declined".into()],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    let plus_ones_column = service
        .create_column(
            receipt::<EditAccessLevel>(database.id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "Plus ones".into(),
                    data_type: DataType::Number,
                    is_multi_select: false,
                    options: vec![],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    world
        .lock()
        .unwrap()
        .grants
        .entry(VIEWER.into())
        .or_default()
        .push((database.id, AccessLevel::View));
    let inserted = service
        .apply_ops(
            receipt::<EditAccessLevel>(database.id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            vec![DatabaseOp::InsertRows {
                table: table_id,
                rows: vec![vec![
                    CellWrite {
                        column: name_column,
                        value: CellValue::Text("Sam".into()),
                    },
                    CellWrite {
                        column: status_column,
                        value: CellValue::Options(vec![OptionRef::Label("Going".into())]),
                    },
                    CellWrite {
                        column: plus_ones_column,
                        value: CellValue::Number(2.0),
                    },
                ]],
                create_missing_options: false,
            }],
        )
        .await
        .unwrap();
    let [OpResult::RowsWritten { inserted, .. }] = inserted.as_slice() else {
        panic!("expected one insert, got {inserted:?}");
    };
    // Tests count the batches their own writes make, not the seed row's.
    world.lock().unwrap().write_batches = 0;
    let column = |id: ColumnId| {
        world
            .lock()
            .unwrap()
            .columns
            .iter()
            .find(|column| column.id == id)
            .unwrap()
            .clone()
    };
    Seeded {
        database_id: database.id,
        table_id,
        row_id: inserted[0],
        name_column: column(name_column),
        status_column: column(status_column),
        plus_ones_column: column(plus_ones_column),
        world,
        service,
    }
}

// ===== Databases =====

#[tokio::test]
async fn create_database_grants_owner_and_starter_table() {
    let world: Shared = Arc::default();
    let svc = service(&world);
    let db = svc
        .create_database(CreateDatabase {
            name: "  Offsite ".into(),
            owner_id: user(OWNER),
            acting_bot: None,
        })
        .await
        .unwrap();
    assert_eq!(db.name, "Offsite");
    let listed = svc.list_databases(viewer(OWNER)).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].grant, AccessLevel::Owner);
    assert_eq!(listed[0].tables.len(), 1);
    assert_eq!(listed[0].tables[0].name, "Table 1");
    assert!(
        svc.list_databases(viewer(STRANGER))
            .await
            .unwrap()
            .is_empty()
    );
    {
        let w = world.lock().unwrap();
        assert_eq!(w.tables.len(), 1);
        assert_eq!(w.tables[0].name, "Table 1");
        assert_eq!(w.tables[0].database_id, db.id);
    }

    let err = svc
        .create_database(CreateDatabase {
            name: "   ".into(),
            owner_id: user(OWNER),
            acting_bot: None,
        })
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::InvalidSchemaOperation(_)));
}

#[tokio::test]
async fn database_details_answer_every_live_database_the_viewer_holds_a_grant_on() {
    let seeded = seeded().await;
    let (svc, offsite, guests) = (seeded.service, seeded.database_id, seeded.table_id);
    let sessions = svc
        .create_table(
            receipt::<EditAccessLevel>(offsite, OWNER, AccessLevel::Owner),
            CreateTable {
                database_id: offsite,
                name: "Sessions".into(),
            },
        )
        .await
        .unwrap();
    svc.reorder_tables(
        receipt::<EditAccessLevel>(offsite, OWNER, AccessLevel::Owner),
        vec![sessions.id, guests],
    )
    .await
    .unwrap();
    let venue = svc
        .create_database(CreateDatabase {
            name: "Venue".into(),
            owner_id: user(OWNER),
            acting_bot: None,
        })
        .await
        .unwrap();
    let archive = svc
        .create_database(CreateDatabase {
            name: "Archive".into(),
            owner_id: user(OWNER),
            acting_bot: None,
        })
        .await
        .unwrap();
    svc.trash_database(receipt::<OwnerAccessLevel>(
        archive.id,
        OWNER,
        AccessLevel::Owner,
    ))
    .await
    .unwrap();

    let details = svc.database_details(viewer(OWNER)).await.unwrap();
    assert_eq!(
        details
            .iter()
            .map(|detail| (detail.database.name.as_str(), detail.grant))
            .collect::<Vec<_>>(),
        vec![
            ("Offsite", AccessLevel::Owner),
            ("Venue", AccessLevel::Owner)
        ],
        "the trashed Archive is left out"
    );
    assert_eq!(
        details[0]
            .tables
            .iter()
            .map(|table| table.table.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Sessions", "Guests"]
    );
    assert!(details[0].tables[0].columns.is_empty());
    assert_eq!(
        details[0].tables[1]
            .columns
            .iter()
            .map(|column| column.definition.definition.display_name.as_str())
            .collect::<Vec<_>>(),
        vec!["Name", "Status", "Plus ones"]
    );
    assert_eq!(details[1].database.id, venue.id);
    assert_eq!(details[1].tables[0].table.name, "Table 1");

    let shared = svc.database_details(viewer(VIEWER)).await.unwrap();
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].database.id, offsite);
    assert_eq!(shared[0].grant, AccessLevel::View);
    assert_eq!(
        shared[0]
            .tables
            .iter()
            .map(|table| table.table.id)
            .collect::<Vec<_>>(),
        vec![sessions.id, guests]
    );

    assert!(
        svc.database_details(viewer(STRANGER))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn rename_validates_the_name_and_writes_it() {
    let seeded = seeded().await;
    let (world, svc, db) = (seeded.world, seeded.service, seeded.database_id);

    let renamed = svc
        .rename_database(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            "  Winter Offsite  ".into(),
        )
        .await
        .unwrap();
    assert_eq!(renamed.name, "Winter Offsite");
    assert_eq!(world.lock().unwrap().databases[0].name, "Winter Offsite");

    let err = svc
        .rename_database(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            "   ".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::InvalidSchemaOperation(_)));

    let err = svc
        .rename_database(
            receipt::<EditAccessLevel>(Uuid::new_v4(), OWNER, AccessLevel::Owner),
            "Elsewhere".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));
}

#[tokio::test]
async fn table_rename_moves_the_sql_name_and_retries_without_overwriting_a_new_name() {
    let seeded = seeded().await;
    let (world, svc, db, table_id) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
    );
    let renamed = svc
        .rename_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Edit),
            table_id,
            "  Attendees  ".into(),
            "Guests".into(),
        )
        .await
        .unwrap();
    assert_eq!(renamed.name, "Attendees");
    assert_eq!(renamed.version, TableVersion(2));
    assert_eq!(
        world.lock().unwrap().published.last(),
        Some(&(table_id, TableVersion(2)))
    );

    let detail = svc
        .get_database(
            receipt::<ViewAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
        )
        .await
        .unwrap();
    assert_eq!(detail.tables[0].sql_name, "\"Offsite\".\"Attendees\"");

    let retried = svc
        .rename_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Edit),
            table_id,
            "Attendees".into(),
            "Guests".into(),
        )
        .await
        .unwrap();
    assert_eq!(retried.version, renamed.version);
    let error = svc
        .rename_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Edit),
            table_id,
            "People".into(),
            "Guests".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DatabaseError::InvalidSchemaOperation(_)));
    assert_eq!(
        world
            .lock()
            .unwrap()
            .tables
            .iter()
            .find(|t| t.id == table_id)
            .unwrap()
            .name,
        "Attendees"
    );
}

#[tokio::test]
async fn table_rename_rejects_invalid_names_foreign_tables_and_trashed_databases() {
    let seeded = seeded().await;
    let (svc, db, table_id) = (seeded.service, seeded.database_id, seeded.table_id);
    svc.create_table(
        receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
        CreateTable {
            database_id: db,
            name: "People".into(),
        },
    )
    .await
    .unwrap();
    for name in [" ", " people "] {
        let error = svc
            .rename_table(
                receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
                table_id,
                name.into(),
                "Guests".into(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, DatabaseError::InvalidSchemaOperation(_)));
    }
    let other = svc
        .create_database(CreateDatabase {
            name: "Elsewhere".into(),
            owner_id: user(OWNER),
            acting_bot: None,
        })
        .await
        .unwrap();
    let error = svc
        .rename_table(
            receipt::<EditAccessLevel>(other.id, OWNER, AccessLevel::Owner),
            table_id,
            "People".into(),
            "Guests".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DatabaseError::NotFound));
    svc.trash_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    let error = svc
        .rename_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            table_id,
            "People".into(),
            "Guests".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DatabaseError::NotFound));
}

#[tokio::test]
async fn trash_hides_the_database_and_restore_brings_it_back() {
    let seeded = seeded().await;
    let (world, svc, db) = (seeded.world, seeded.service, seeded.database_id);

    svc.trash_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    let trashed_at = world.lock().unwrap().databases[0].trashed_at;
    assert!(trashed_at.is_some());

    // A trashed database is invisible to listing, reads, ops, and renames.
    assert!(svc.list_databases(viewer(OWNER)).await.unwrap().is_empty());
    let err = svc
        .get_database(
            receipt::<ViewAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));
    let err = svc
        .apply_ops(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            vec![DatabaseOp::DeleteRows {
                table: seeded.table_id,
                rows: vec![seeded.row_id],
            }],
        )
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound), "{err:?}");
    let err = svc
        .rename_database(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            "Renamed".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));

    // Trashing again keeps the original timestamp.
    svc.trash_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    assert_eq!(world.lock().unwrap().databases[0].trashed_at, trashed_at);

    svc.restore_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    assert!(world.lock().unwrap().databases[0].trashed_at.is_none());
    assert_eq!(svc.list_databases(viewer(OWNER)).await.unwrap().len(), 1);

    // Restoring a live database is a no-op, not an error.
    svc.restore_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
}

#[tokio::test]
async fn permanent_delete_removes_the_database_its_rows_and_its_grants() {
    let seeded = seeded().await;
    let (world, svc, db, row_id) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.row_id,
    );

    svc.delete_database_permanently(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();

    {
        let w = world.lock().unwrap();
        assert!(w.databases.is_empty());
        assert!(w.tables.is_empty());
        assert!(w.rows.is_empty());
        assert!(!w.cells.contains_key(&row_id));
        assert!(w.grants.values().all(|grants| grants.is_empty()));
    }
    assert!(svc.list_databases(viewer(VIEWER)).await.unwrap().is_empty());

    let err = svc
        .delete_database_permanently(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound));
}

#[tokio::test]
async fn lifecycle_operations_act_on_trashed_databases() {
    let seeded = seeded().await;
    let (world, svc, db) = (seeded.world, seeded.service, seeded.database_id);
    svc.trash_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();

    svc.delete_database_permanently(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();

    assert!(world.lock().unwrap().databases.is_empty());
}

#[tokio::test]
async fn schema_operations_respect_receipts() {
    let seeded = seeded().await;
    let (svc, db, table_id) = (seeded.service, seeded.database_id, seeded.table_id);
    let other = Uuid::new_v4();
    let err = svc
        .create_table(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            CreateTable {
                database_id: other,
                name: "Nope".into(),
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::Unauthorized));

    let err = svc
        .create_column(
            receipt::<EditAccessLevel>(other, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "X".into(),
                    data_type: DataType::String,
                    is_multi_select: false,
                    options: vec![],
                },
                config: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, DatabaseError::NotFound),
        "a table outside the receipted database looks missing"
    );

    let detail = svc
        .get_database(
            receipt::<ViewAccessLevel>(db, VIEWER, AccessLevel::View),
            viewer(VIEWER),
        )
        .await
        .unwrap();
    assert_eq!(detail.grant, AccessLevel::View);
    assert_eq!(detail.tables.len(), 1);
    assert_eq!(detail.tables[0].sql_name, "\"Offsite\".\"Guests\"");
    assert_eq!(detail.tables[0].read_sql_name, "\"Offsite\".\"Guests\"");
    assert_eq!(
        detail.tables[0]
            .columns
            .iter()
            .map(|column| column.sql_name.as_str())
            .collect::<Vec<_>>(),
        vec!["\"Name\"", "\"Status\"", "\"Plus ones\""]
    );
    assert!(detail.tables[0].columns.iter().all(|c| !c.writable));

    let detail = svc
        .get_database(
            receipt::<ViewAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
        )
        .await
        .unwrap();
    assert_eq!(detail.grant, AccessLevel::Owner);
    assert!(detail.tables[0].columns.iter().all(|c| c.writable));
}

// ===== Row writes =====

#[tokio::test]
async fn a_write_to_a_row_of_another_table_is_refused() {
    let seeded = seeded().await;
    let (world, svc, database_id, table_id, row_id) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.row_id,
    );
    let sessions = svc
        .create_table(
            receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner),
            CreateTable {
                database_id,
                name: "Sessions".into(),
            },
        )
        .await
        .unwrap();
    let title = svc
        .create_column(
            receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: sessions.id,
                binding: ColumnBinding::NewDefinition {
                    name: "Title".into(),
                    data_type: DataType::String,
                    is_multi_select: false,
                    options: vec![],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    let cells_before = world.lock().unwrap().cells.clone();
    let sessions_version = table_version(&world, sessions.id);

    let error = svc
        .apply_ops(
            receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            vec![DatabaseOp::UpdateRows {
                table: sessions.id,
                changes: RowChanges::Uniform {
                    rows: vec![row_id],
                    cells: vec![CellWrite {
                        column: title,
                        value: CellValue::Text("Hijacked".into()),
                    }],
                },
                create_missing_options: false,
            }],
        )
        .await
        .unwrap_err();
    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    assert_eq!(
        refusal,
        OpRefusal {
            op: 0,
            row: Some(0),
            column: None,
            reason: format!("no row {row_id} in this table"),
        }
    );
    let error = svc
        .apply_ops(
            receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            vec![DatabaseOp::DeleteRows {
                table: sessions.id,
                rows: vec![row_id],
            }],
        )
        .await
        .unwrap_err();
    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    assert_eq!(
        refusal,
        OpRefusal {
            op: 0,
            row: Some(0),
            column: None,
            reason: format!("no row {row_id} in this table"),
        }
    );

    assert_eq!(world.lock().unwrap().cells, cells_before);
    assert_eq!(row_ids(&world, table_id), vec![row_id]);
    assert_eq!(table_version(&world, sessions.id), sessions_version);
}

#[tokio::test]
async fn a_view_grant_reads_but_cannot_be_receipted_for_ops() {
    let seeded = seeded().await;

    let edit = EntityAccessReceipt::<EditAccessLevel>::try_new_authenticated_user(
        user(VIEWER),
        Entity {
            entity_id: seeded.database_id.to_string(),
            entity_type: EntityType::Database,
        },
        EntityPermission::AccessLevel {
            access_level: AccessLevel::View,
        },
    );
    assert!(edit.is_err(), "ops need an edit receipt");

    let detail = seeded
        .service
        .get_database(
            receipt::<ViewAccessLevel>(seeded.database_id, VIEWER, AccessLevel::View),
            viewer(VIEWER),
        )
        .await
        .unwrap();
    assert_eq!(detail.grant, AccessLevel::View);
    assert!(
        detail.tables[0]
            .columns
            .iter()
            .all(|column| !column.writable)
    );
}

#[tokio::test]
async fn grants_scope_writes_per_database() {
    let seeded = seeded().await;
    let (world, svc) = (seeded.world, seeded.service);
    let venue = svc
        .create_database(CreateDatabase {
            name: "Venue".into(),
            owner_id: user(VIEWER),
            acting_bot: None,
        })
        .await
        .unwrap();
    let rooms = {
        let mut w = world.lock().unwrap();
        let table = w
            .tables
            .iter_mut()
            .find(|t| t.database_id == venue.id)
            .unwrap();
        table.name = "Rooms".into();
        table.id
    };
    let room_name = svc
        .create_column(
            receipt::<EditAccessLevel>(venue.id, VIEWER, AccessLevel::Owner),
            viewer(VIEWER),
            CreateColumn {
                infer_type: false,
                table_id: rooms,
                binding: ColumnBinding::NewDefinition {
                    name: "Name".into(),
                    data_type: DataType::String,
                    is_multi_select: false,
                    options: vec![],
                },
                config: None,
            },
        )
        .await
        .unwrap();

    // VIEWER writes their own database...
    let written = svc
        .apply_ops(
            receipt::<EditAccessLevel>(venue.id, VIEWER, AccessLevel::Owner),
            viewer(VIEWER),
            vec![DatabaseOp::InsertRows {
                table: rooms,
                rows: vec![vec![CellWrite {
                    column: room_name,
                    value: CellValue::Text("Main Hall".into()),
                }]],
                create_missing_options: false,
            }],
        )
        .await
        .unwrap();
    assert!(matches!(
        written.as_slice(),
        [OpResult::RowsWritten {
            table_version: TableVersion(1),
            affected: 1,
            ..
        }]
    ));

    // ...but their receipt on it reaches no table of OWNER's.
    let error = svc
        .apply_ops(
            receipt::<EditAccessLevel>(venue.id, VIEWER, AccessLevel::Owner),
            viewer(VIEWER),
            vec![DatabaseOp::InsertRows {
                table: seeded.table_id,
                rows: vec![vec![]],
                create_missing_options: false,
            }],
        )
        .await
        .unwrap_err();
    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    assert_eq!(
        refusal.reason,
        format!("table {} is not in this database", seeded.table_id)
    );

    // OWNER has no grant on Venue: it is not theirs to list.
    let listed = svc.list_databases(viewer(OWNER)).await.unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|listed| listed.database.id)
            .collect::<Vec<_>>(),
        vec![seeded.database_id]
    );
    let w = world.lock().unwrap();
    assert_eq!(w.rows[&rooms].len(), 1);
    assert_eq!(w.rows[&seeded.table_id].len(), 1);
}

// ===== Select options are explicit schema =====

#[tokio::test]
async fn a_select_column_with_no_options_accepts_nothing() {
    let seeded = seeded().await;
    let (svc, database_id, table_id) = (seeded.service, seeded.database_id, seeded.table_id);
    let stage = svc
        .create_column(
            receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "Stage".into(),
                    data_type: DataType::SelectString,
                    is_multi_select: false,
                    options: vec![],
                },
                config: None,
            },
        )
        .await
        .unwrap();

    let error = svc
        .apply_ops(
            receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            vec![DatabaseOp::InsertRows {
                table: table_id,
                rows: vec![vec![CellWrite {
                    column: stage,
                    value: CellValue::Options(vec![OptionRef::Label("Main".into())]),
                }]],
                create_missing_options: false,
            }],
        )
        .await
        .unwrap_err();
    let DatabaseError::InvalidOp(refusal) = error else {
        panic!("expected a refused op, got {error:?}");
    };
    assert_eq!(refusal.reason, "`Main` is not an option of \"Stage\"");
}

/// The point of the operation: the write that failed succeeds once the option
/// exists, and the table's version moves because its schema did.
#[tokio::test]
async fn add_column_options_extends_what_ops_accept_and_bumps_the_version() {
    let seeded = seeded().await;
    let (world, svc, db, guests, status) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.status_column.id,
    );
    let published_before = world.lock().unwrap().published.len();

    let column = svc
        .add_column_options(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            AddColumnOptions {
                table_id: guests,
                column_id: status,
                labels: vec!["Waitlisted".into()],
            },
        )
        .await
        .expect("edit access may extend a select column");

    assert_eq!(column.sql_name, "\"Status\"");
    assert_eq!(
        catalog::option_labels(&column.definition)
            .into_iter()
            .map(|(_, label)| label)
            .collect::<Vec<_>>(),
        vec!["Going", "Declined", "Waitlisted"],
        "new options are appended, so existing labels do not move"
    );

    {
        let w = world.lock().unwrap();
        assert_eq!(w.tables[0].version, TableVersion(2));
        assert_eq!(
            w.published.len(),
            published_before + 1,
            "the schema change is announced for liveness"
        );
    }

    let inserted = svc
        .apply_ops(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            vec![DatabaseOp::InsertRows {
                table: guests,
                rows: vec![vec![CellWrite {
                    column: status,
                    value: CellValue::Options(vec![OptionRef::Label("Waitlisted".into())]),
                }]],
                create_missing_options: false,
            }],
        )
        .await
        .expect("the option now resolves");
    let [OpResult::RowsWritten { inserted, .. }] = inserted.as_slice() else {
        panic!("expected one insert, got {inserted:?}");
    };
    let waitlisted = option_id(&world, column.definition.definition.id, "Waitlisted");
    assert_eq!(
        cell(&world, inserted[0], column.definition.definition.id),
        Some(PropertyValue::SelectOption(vec![waitlisted]))
    );
}

/// Re-sending a label the column already has changes nothing: no duplicate
/// option, no version bump, no event — and no error either.
#[tokio::test]
async fn adding_an_existing_option_is_a_no_op() {
    let seeded = seeded().await;
    let (world, svc, db, guests, status) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.status_column.id,
    );
    let published_before = world.lock().unwrap().published.len();

    let column = svc
        .add_column_options(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            AddColumnOptions {
                table_id: guests,
                column_id: status,
                labels: vec!["going".into(), "  Declined  ".into()],
            },
        )
        .await
        .expect("an option that is already there is not an error");

    assert_eq!(column.definition.property_options.len(), 2);
    let w = world.lock().unwrap();
    assert_eq!(w.tables[0].version, TableVersion(1));
    assert_eq!(w.published.len(), published_before);
}

#[tokio::test]
async fn options_are_refused_on_a_column_that_cannot_hold_them() {
    let seeded = seeded().await;
    let (svc, db, guests, name_column) = (
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.name_column.id,
    );

    let err = svc
        .create_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: guests,
                binding: ColumnBinding::NewDefinition {
                    name: "Notes".into(),
                    data_type: DataType::String,
                    is_multi_select: false,
                    options: vec!["Main".into()],
                },
                config: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            DatabaseError::InvalidSchemaOperation(SchemaError::OptionsOnPlainColumn)
        ),
        "{err:?}"
    );

    // …and the same on the standalone operation, against the text column the
    // seeded table already has.
    let err = svc
        .add_column_options(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            AddColumnOptions {
                table_id: guests,
                column_id: name_column,
                labels: vec!["Main".into()],
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            DatabaseError::InvalidSchemaOperation(SchemaError::ColumnTakesNoOptions)
        ),
        "{err:?}"
    );
}

/// A numeric select stores numbers, so its labels have to be numbers, and
/// labels naming the same number are one option.
#[tokio::test]
async fn numeric_select_options_are_parsed_as_numbers() {
    let seeded = seeded().await;
    let (world, svc, db, guests) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
    );

    let err = svc
        .create_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: guests,
                binding: ColumnBinding::NewDefinition {
                    name: "Priority".into(),
                    data_type: DataType::SelectNumber,
                    is_multi_select: false,
                    options: vec!["soon".into()],
                },
                config: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, DatabaseError::InvalidSchemaOperation(SchemaError::OptionNotNumber { label }) if label == "soon"),
        "{err:?}"
    );

    let priority = svc
        .create_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: guests,
                binding: ColumnBinding::NewDefinition {
                    name: "Priority".into(),
                    data_type: DataType::SelectNumber,
                    is_multi_select: false,
                    options: vec!["1".into(), "2.0".into(), "2".into()],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    let w = world.lock().unwrap();
    let definition = w
        .columns
        .iter()
        .find(|column| column.id == priority)
        .unwrap()
        .property_definition_id;
    assert_eq!(
        w.definitions[&definition]
            .property_options
            .iter()
            .map(|option| option.value.clone())
            .collect::<Vec<_>>(),
        vec![
            PropertyOptionValue::Number(1.0),
            PropertyOptionValue::Number(2.0)
        ]
    );
}

#[tokio::test]
async fn option_labels_are_validated() {
    let seeded = seeded().await;
    let (svc, db, guests) = (seeded.service, seeded.database_id, seeded.table_id);

    for bad in ["   ", ""] {
        let err = svc
            .create_column(
                receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
                viewer(OWNER),
                CreateColumn {
                    infer_type: false,
                    table_id: guests,
                    binding: ColumnBinding::NewDefinition {
                        name: "Stage".into(),
                        data_type: DataType::SelectString,
                        is_multi_select: false,
                        options: vec![bad.into()],
                    },
                    config: None,
                },
            )
            .await
            .unwrap_err();
        assert!(
            matches!(
                err,
                DatabaseError::InvalidSchemaOperation(SchemaError::EmptyOptionLabel)
            ),
            "{err:?}"
        );
    }

    let err = svc
        .create_column(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: guests,
                binding: ColumnBinding::NewDefinition {
                    name: "Stage".into(),
                    data_type: DataType::SelectString,
                    is_multi_select: false,
                    options: vec!["x".repeat(MAX_OPTION_LABEL_LEN + 1)],
                },
                config: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            DatabaseError::InvalidSchemaOperation(SchemaError::OptionLabelTooLong {
                max: MAX_OPTION_LABEL_LEN
            })
        ),
        "{err:?}"
    );
}

#[tokio::test]
async fn add_column_options_respects_receipts() {
    let seeded = seeded().await;
    let (world, svc, db, guests, status) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.status_column.id,
    );
    let elsewhere = Uuid::new_v4();

    let err = svc
        .add_column_options(
            receipt::<EditAccessLevel>(elsewhere, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            AddColumnOptions {
                table_id: guests,
                column_id: status,
                labels: vec!["Waitlisted".into()],
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, DatabaseError::NotFound),
        "a receipt for another database reaches nothing: {err:?}"
    );

    let err = svc
        .add_column_options(
            receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
            viewer(OWNER),
            AddColumnOptions {
                table_id: guests,
                column_id: Uuid::new_v4(),
                labels: vec!["Waitlisted".into()],
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, DatabaseError::NotFound), "{err:?}");

    // Nothing was written on the way to either refusal.
    assert_eq!(world.lock().unwrap().definitions.len(), 3);
}

// ===== Domain events =====

#[tokio::test]
async fn lifecycle_and_writes_publish_domain_events() {
    let seeded = seeded().await;
    let (world, svc, db, row_id) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.row_id,
    );

    // Seeding created the database and then shaped its table.
    {
        let w = world.lock().unwrap();
        assert_eq!(w.broker_events[0]["event_type"], "database.created");
        assert!(
            w.broker_events[1..]
                .iter()
                .all(|event| event["event_type"] == "database.tables_changed"),
            "{:?}",
            w.broker_events
        );
    }
    world.lock().unwrap().broker_events.clear();

    svc.rename_database(
        receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
        "Winter Offsite".into(),
    )
    .await
    .unwrap();
    svc.apply_ops(
        receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
        viewer(OWNER),
        vec![DatabaseOp::UpdateRows {
            table: seeded.table_id,
            changes: RowChanges::Uniform {
                rows: vec![row_id],
                cells: vec![CellWrite {
                    column: seeded.status_column.id,
                    value: CellValue::Options(vec![OptionRef::Label("Declined".into())]),
                }],
            },
            create_missing_options: false,
        }],
    )
    .await
    .unwrap();
    svc.trash_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    svc.restore_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    svc.delete_database_permanently(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();

    let events = world.lock().unwrap().broker_events.clone();
    assert_eq!(
        events
            .iter()
            .map(|event| event["event_type"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "database.renamed",
            "database.tables_changed",
            "database.trashed",
            "database.restored",
            "database.purged",
        ]
    );
    let renamed = &events[0]["metadata"];
    assert_eq!(renamed["database_id"], db.to_string());
    assert_eq!(renamed["name"], "Winter Offsite");
    assert_eq!(renamed["attribution"]["actor"], OWNER);
    let changed = &events[1]["metadata"];
    assert_eq!(changed["database_id"], db.to_string());
    assert_eq!(changed["attribution"]["actor"], OWNER);
    assert_eq!(
        changed["tables"],
        serde_json::json!([{ "table_id": seeded.table_id, "version": 2 }])
    );
    assert_eq!(events[4]["metadata"]["database_id"], db.to_string());
}

#[tokio::test]
async fn an_agent_is_attributed_as_acting_for_the_user() {
    let seeded = seeded().await;
    let (world, svc, db, row_id) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.row_id,
    );
    world.lock().unwrap().broker_events.clear();
    let agent = bot_id::MACRO_AI_BOT_ID;

    svc.create_database(CreateDatabase {
        name: "Agent Offsite".into(),
        owner_id: user(OWNER),
        acting_bot: Some(agent),
    })
    .await
    .unwrap();
    svc.rename_database(
        EntityAccessReceipt::try_new_bot(
            agent.into_storage_id(),
            (&entity_access::domain::models::BotAccessScope::user(user(OWNER))).into(),
            Entity {
                entity_id: db.to_string(),
                entity_type: EntityType::Database,
            },
            EntityPermission::AccessLevel {
                access_level: AccessLevel::Owner,
            },
        )
        .unwrap(),
        "Winter Offsite".into(),
    )
    .await
    .unwrap();
    svc.apply_ops(
        receipt::<EditAccessLevel>(db, OWNER, AccessLevel::Owner),
        Viewer {
            user_id: user(OWNER),
            acting_bot: Some(agent),
        },
        vec![DatabaseOp::UpdateRows {
            table: seeded.table_id,
            changes: RowChanges::Uniform {
                rows: vec![row_id],
                cells: vec![CellWrite {
                    column: seeded.status_column.id,
                    value: CellValue::Options(vec![OptionRef::Label("Declined".into())]),
                }],
            },
            create_missing_options: false,
        }],
    )
    .await
    .unwrap();

    let events = world.lock().unwrap().broker_events.clone();
    assert_eq!(
        events
            .iter()
            .map(|event| event["event_type"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "database.created",
            "database.renamed",
            "database.tables_changed"
        ]
    );
    for event in &events {
        assert_eq!(
            event["metadata"]["attribution"],
            serde_json::json!({
                "actor": "bot|00000000-0000-0000-0000-00000000a1a1",
                "on_behalf_of": OWNER,
            }),
            "{event}"
        );
    }
}

#[tokio::test]
async fn no_op_lifecycle_calls_publish_nothing() {
    let seeded = seeded().await;
    let (world, svc, db) = (seeded.world, seeded.service, seeded.database_id);
    svc.trash_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    world.lock().unwrap().broker_events.clear();

    // Trashing twice and restoring what is not trashed change nothing, so
    // nothing is announced.
    svc.trash_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    svc.restore_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    svc.restore_database(receipt::<OwnerAccessLevel>(db, OWNER, AccessLevel::Owner))
        .await
        .unwrap();
    let events = world.lock().unwrap().broker_events.clone();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["event_type"], "database.restored");
}
