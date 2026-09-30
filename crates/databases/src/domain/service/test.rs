//! Service tests: the real `database_sql` engine over in-memory fakes for
//! every port, so the whole exec pipeline (catalog → compile → fetch → fold
//! → write) is exercised without Postgres, and every allow/deny decision is
//! asserted at the service boundary.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use entity_access::domain::models::{
    AccessLevel, Entity, EntityAccessReceipt, EntityPermission, EntityType, RequiredPermission,
};
use macro_event_broker::{EventBrokerError, MacroEvent, MacroEventBroker};
use macro_user_id::user_id::MacroUserIdStr;
use models_properties::service::property_definition::PropertyDefinition;
use models_properties::service::property_definition_with_options::PropertyDefinitionWithOptions;
use models_properties::service::property_option::{PropertyOption, PropertyOptionValue};
use models_properties::service::property_value::PropertyValue;
use models_properties::shared::{DataType, EntityType as PropertyEntityType, PropertyOwner};
use uuid::Uuid;

use super::*;
use crate::domain::models::{
    Column, ColumnBinding, ColumnConfig, PropertyDefinitionId, RowId, RowRef, SqlValue,
    TableDeletion, TableOrderOutcome, TableVersion,
};

mod casts;
mod columns;
mod delete_table;
mod discovery;
mod infer_column_type;
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
    type Err = FakeError;
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
        let row_ids = w
            .rows
            .remove(&table.id)
            .unwrap_or_default()
            .into_iter()
            .map(|row| row.id)
            .collect();
        Ok(TableDeletion::Deleted { row_ids })
    }
    async fn create_column(
        &self,
        table_id: TableId,
        property_definition_id: PropertyDefinitionId,
        cmd: &CreateColumn,
    ) -> Result<ColumnId, FakeError> {
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
        Ok(column.id)
    }
    async fn bump_table_version(&self, table_id: TableId) -> Result<TableVersion, FakeError> {
        let mut w = self.0.lock().unwrap();
        let table = w
            .tables
            .iter_mut()
            .find(|t| t.id == table_id)
            .ok_or(FakeError)?;
        table.version = TableVersion(table.version.0 + 1);
        Ok(table.version)
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
    async fn replace_column(
        &self,
        table: &Table,
        replacement: &ColumnReplacement,
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
        w.columns[c].property_definition_id = replacement.definition_id;
        w.columns[c].config = replacement.config.clone();
        w.columns[c].infer_type = false;
        w.tables[t].version.0 += 1;
        Ok(Some(w.tables[t].version))
    }
    async fn delete_column(
        &self,
        table: &Table,
        column: &Column,
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
        w.columns.remove(c);
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
    async fn insert_rows(
        &self,
        table_id: TableId,
        _created_by: &str,
        count: usize,
    ) -> Result<Option<Vec<RowRef>>, FakeError> {
        let mut w = self.0.lock().unwrap();
        let Some(table) = w.tables.iter().find(|t| t.id == table_id) else {
            return Ok(None);
        };
        let database_id = table.database_id;
        if w.databases
            .iter()
            .find(|d| d.id == database_id)
            .is_none_or(|d| d.trashed_at.is_some())
        {
            return Ok(None);
        }
        let rows = w.rows.entry(table_id).or_default();
        let created: Vec<RowRef> = (0..count)
            .map(|offset| RowRef {
                id: Uuid::now_v7(),
                position: format!("{:04}", rows.len() + offset),
            })
            .collect();
        rows.extend(created.iter().cloned());
        Ok(Some(created))
    }
    async fn delete_row(&self, table_id: TableId, row_id: RowId) -> Result<bool, FakeError> {
        let mut w = self.0.lock().unwrap();
        let Some(rows) = w.rows.get_mut(&table_id) else {
            return Ok(false);
        };
        let before = rows.len();
        rows.retain(|row| row.id != row_id);
        Ok(rows.len() < before)
    }
    async fn row_table(&self, row_id: RowId) -> Result<Option<TableId>, FakeError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .rows
            .iter()
            .find(|(_, rows)| rows.iter().any(|row| row.id == row_id))
            .map(|(table_id, _)| *table_id))
    }
    async fn settle_inference(
        &self,
        table_id: TableId,
        definitions: &[PropertyDefinitionId],
    ) -> Result<(), FakeError> {
        let mut w = self.0.lock().unwrap();
        for column in &mut w.columns {
            if column.table_id == table_id && definitions.contains(&column.property_definition_id) {
                column.infer_type = false;
            }
        }
        w.settled.push((table_id, definitions.to_vec()));
        Ok(())
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
    async fn save_query(
        &self,
        database_id: Option<DatabaseId>,
        definition: &QueryDefinition,
        created_by: &str,
    ) -> Result<SavedQuery, FakeError> {
        let saved = SavedQuery {
            id: Uuid::now_v7(),
            definition: definition.clone(),
            database_id,
            created_by: created_by.to_string(),
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
    type Err = FakeError;
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
    async fn write(
        &self,
        row: RowId,
        cells: &[(PropertyDefinitionId, Option<PropertyValue>)],
    ) -> Result<(), FakeError> {
        let mut w = self.0.lock().unwrap();
        let stored = w.cells.entry(row).or_default();
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
        Ok(())
    }
    async fn clear(&self, row: RowId) -> Result<(), FakeError> {
        self.0.lock().unwrap().cells.remove(&row);
        Ok(())
    }
}

impl ColumnDefinitionStore for FakeDefs {
    type Err = FakeError;
    async fn resolve_binding(
        &self,
        database_id: DatabaseId,
        _viewer: &Viewer,
        binding: &ColumnBinding,
    ) -> Result<PropertyDefinitionId, FakeError> {
        match binding {
            ColumnBinding::ExistingDefinition(id) => {
                if self.0.lock().unwrap().definitions.contains_key(id) {
                    Ok(*id)
                } else {
                    Err(FakeError)
                }
            }
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
                Ok(id)
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
}

impl TableEventPublisher for FakeEvents {
    type Err = FakeError;
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
        user_id: &str,
        state: &Awareness,
    ) -> Result<(), FakeError> {
        self.0
            .lock()
            .unwrap()
            .awareness
            .push((database_id, user_id.to_string(), state.clone()));
        Ok(())
    }
}

impl AccessDirectory for FakeAccess {
    type Err = FakeError;
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
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: "INSERT INTO guests (name, status, \"Plus ones\") VALUES ('Sam', 'Going', 2)"
                    .into(),
                base_versions: None,
            },
        )
        .await
        .unwrap();
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
        row_id: inserted.inserted_row_ids[0],
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

    let answer = svc
        .query_sql(viewer(OWNER), "SELECT name FROM attendees".into())
        .await
        .unwrap();
    assert_eq!(answer.results[0].rows.len(), 1);
    assert_eq!(answer.results[0].rows[0][1], SqlValue::Text("Sam".into()));
    let error = svc
        .query_sql(viewer(OWNER), "SELECT name FROM guests".into())
        .await
        .unwrap_err();
    assert!(
        matches!(error, QueryError::Sql(ref message) if message == "unknown table guests"),
        "{error:?}"
    );

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

    // A trashed database is invisible to listing, reads, SQL, and renames.
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
        .query_sql(viewer(OWNER), "SELECT name FROM guests".into())
        .await
        .unwrap_err();
    assert!(
        matches!(err, QueryError::Sql(ref message) if message == "unknown table guests"),
        "{err:?}"
    );
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

// ===== SQL: reads =====

#[tokio::test]
async fn select_answers_row_id_first_and_maps_every_value_kind() {
    let seeded = seeded().await;
    let (world, svc, database_id, table_id, row_id) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.row_id,
    );
    svc.create_column(
        receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner),
        viewer(OWNER),
        CreateColumn {
            infer_type: false,
            table_id,
            binding: ColumnBinding::NewDefinition {
                name: "Confirmed".into(),
                data_type: DataType::Boolean,
                is_multi_select: false,
                options: vec![],
            },
            config: None,
        },
    )
    .await
    .unwrap();
    svc.create_column(
        receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner),
        viewer(OWNER),
        CreateColumn {
            infer_type: false,
            table_id,
            binding: ColumnBinding::NewDefinition {
                name: "Tags".into(),
                data_type: DataType::Tag,
                is_multi_select: true,
                options: vec!["vip".into(), "speaker".into()],
            },
            config: None,
        },
    )
    .await
    .unwrap();
    svc.exec_sql(
        viewer(OWNER),
        ExecRequest {
            scope: None,
            sql: format!(
                "UPDATE guests SET confirmed = TRUE, tags = ['speaker', 'vip'] WHERE row_id = '{row_id}'"
            ),
            base_versions: None,
        },
    )
    .await
    .unwrap();

    for caller in [OWNER, VIEWER] {
        let outcome = svc
            .query_sql(viewer(caller), "SELECT * FROM guests".into())
            .await
            .unwrap();
        assert_eq!(outcome.results.len(), 1);
        let result = &outcome.results[0];
        assert_eq!(
            result
                .columns
                .iter()
                .map(|column| column.name.as_str())
                .collect::<Vec<_>>(),
            vec!["row_id", "Name", "Status", "Plus ones", "Confirmed", "Tags"]
        );
        assert_eq!(result.columns[0].origin, None);
        assert_eq!(
            result.columns[1].origin,
            Some(("Guests".to_string(), "Name".to_string()))
        );
        assert_eq!(
            result.rows,
            vec![vec![
                SqlValue::Text(row_id.to_string()),
                SqlValue::Text("Sam".into()),
                SqlValue::Text("Going".into()),
                SqlValue::Real(2.0),
                SqlValue::Integer(1),
                SqlValue::Text("[\"speaker\",\"vip\"]".into()),
            ]]
        );
        assert_eq!(outcome.changes_applied, 0);
        assert!(outcome.new_versions.is_empty());
        assert!(outcome.inserted_row_ids.is_empty());
        assert_eq!(outcome.read_tables, vec![table_id]);
        assert_eq!(outcome.read_database_ids, vec![database_id]);
        assert_eq!(
            outcome.read_versions,
            HashMap::from([(table_id, world.lock().unwrap().tables[0].version)])
        );
        assert!(outcome.truncated_tables.is_empty());
    }
}

#[tokio::test]
async fn row_position_orders_rows_by_their_stored_position() {
    let seeded = seeded().await;
    let (svc, world, table_id) = (seeded.service, seeded.world, seeded.table_id);
    svc.exec_sql(
        viewer(OWNER),
        ExecRequest {
            scope: None,
            sql: "INSERT INTO guests (name) VALUES ('Ada'), ('Bo')".into(),
            base_versions: None,
        },
    )
    .await
    .unwrap();
    // The store lists Sam, Ada, Bo; their positions put Bo first and Sam last.
    for (row, position) in world
        .lock()
        .unwrap()
        .rows
        .get_mut(&table_id)
        .unwrap()
        .iter_mut()
        .zip(["0003", "0002", "0001"])
    {
        row.position = position.into();
    }

    let ascending = svc
        .query_sql(
            viewer(OWNER),
            "SELECT name FROM guests ORDER BY row_position".into(),
        )
        .await
        .unwrap();
    assert_eq!(
        ascending.results[0]
            .rows
            .iter()
            .map(|row| row[1].clone())
            .collect::<Vec<_>>(),
        vec![
            SqlValue::Text("Bo".into()),
            SqlValue::Text("Ada".into()),
            SqlValue::Text("Sam".into()),
        ]
    );

    let descending = svc
        .query_sql(
            viewer(OWNER),
            "SELECT name, row_position FROM guests ORDER BY row_position DESC".into(),
        )
        .await
        .unwrap();
    assert_eq!(
        descending.results[0]
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        vec!["row_id", "Name", "row_position"]
    );
    assert_eq!(
        descending.results[0]
            .rows
            .iter()
            .map(|row| row[1..].to_vec())
            .collect::<Vec<_>>(),
        vec![
            vec![SqlValue::Text("Sam".into()), SqlValue::Text("0003".into())],
            vec![SqlValue::Text("Ada".into()), SqlValue::Text("0002".into())],
            vec![SqlValue::Text("Bo".into()), SqlValue::Text("0001".into())],
        ]
    );
}

#[tokio::test]
async fn select_filters_orders_and_counts_by_group() {
    let seeded = seeded().await;
    let (svc, table_id) = (seeded.service, seeded.table_id);
    svc.exec_sql(
        viewer(OWNER),
        ExecRequest {
            scope: None,
            sql: "INSERT INTO guests (name, status, \"Plus ones\") VALUES ('Ada', 'Declined', 0), ('Bo', 'Going', 1), ('Cy', NULL, 3)".into(),
            base_versions: None,
        },
    )
    .await
    .unwrap();

    // A select option pushes down to the store; text and numbers fold here.
    let going = svc
        .query_sql(
            viewer(OWNER),
            "SELECT name, \"Plus ones\" FROM guests WHERE status = 'Going' AND \"Plus ones\" > 1 ORDER BY name DESC".into(),
        )
        .await
        .unwrap();
    assert_eq!(
        going.results[0]
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        vec!["row_id", "Name", "Plus ones"]
    );
    assert_eq!(
        going.results[0]
            .rows
            .iter()
            .map(|row| row[1..].to_vec())
            .collect::<Vec<_>>(),
        vec![vec![SqlValue::Text("Sam".into()), SqlValue::Real(2.0)]]
    );

    let ordered = svc
        .query_sql(
            viewer(OWNER),
            "SELECT name FROM guests WHERE name LIKE '%a%' OR status IS NULL ORDER BY \"Plus ones\" DESC LIMIT 2".into(),
        )
        .await
        .unwrap();
    assert_eq!(
        ordered.results[0]
            .rows
            .iter()
            .map(|row| row[1].clone())
            .collect::<Vec<_>>(),
        vec![SqlValue::Text("Cy".into()), SqlValue::Text("Sam".into())]
    );

    // Aggregates are not row-shaped: no `row_id`, numbers as reals.
    let total = svc
        .query_sql(viewer(OWNER), "SELECT COUNT(*) FROM guests".into())
        .await
        .unwrap();
    assert_eq!(total.results[0].columns.len(), 1);
    assert_eq!(total.results[0].columns[0].name, "COUNT(*)");
    assert_eq!(total.results[0].rows, vec![vec![SqlValue::Real(4.0)]]);
    assert_eq!(total.read_tables, vec![table_id]);

    let by_status = svc
        .query_sql(
            viewer(OWNER),
            "SELECT status, COUNT(*) FROM guests GROUP BY status ORDER BY status".into(),
        )
        .await
        .unwrap();
    assert_eq!(
        by_status.results[0]
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Status", "COUNT(*)"]
    );
    assert_eq!(
        by_status.results[0].rows,
        vec![
            vec![SqlValue::Text("Going".into()), SqlValue::Real(2.0)],
            vec![SqlValue::Text("Declined".into()), SqlValue::Real(1.0)],
            vec![SqlValue::Null, SqlValue::Real(1.0)],
        ]
    );
}

#[tokio::test]
async fn compile_errors_surface_the_engines_message() {
    let seeded = seeded().await;
    let (world, svc, row_id) = (seeded.world, seeded.service, seeded.row_id);
    let version = world.lock().unwrap().tables[0].version;

    let error = svc
        .query_sql(viewer(OWNER), "SELECT nam FROM guests".into())
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            QueryError::Sql(ref message)
                if message == "unknown column \"nam\" in Offsite.Guests — did you mean \"Name\"?"
        ),
        "{error:?}"
    );

    let error = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: "INSERT INTO guests (name, status) VALUES ('Bo', 'Waitlisted')".into(),
                base_versions: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            QueryError::Sql(ref message)
                if message == "\"Waitlisted\" is not an option of \"Status\" (Going, Declined)"
        ),
        "{error:?}"
    );

    let error = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: format!(
                    "UPDATE guests SET status = ['Going', 'Declined'] WHERE row_id = '{row_id}'"
                ),
                base_versions: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            QueryError::Sql(ref message)
                if message == "\"Status\" holds one value; a list of 2 was given"
        ),
        "{error:?}"
    );

    let error = svc
        .query_sql(
            viewer(OWNER),
            "SELECT name FROM guests WHERE \"Plus ones\" = 'two'".into(),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            QueryError::Sql(ref message)
                if message == "\"Plus ones\" is a number column; compare it to a number"
        ),
        "{error:?}"
    );

    let error = svc
        .query_sql(
            viewer(OWNER),
            "SELECT name FROM guests; DROP TABLE guests".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, QueryError::Sql(_)), "{error:?}");

    let w = world.lock().unwrap();
    assert_eq!(w.tables[0].version, version);
    assert_eq!(w.rows.values().map(Vec::len).sum::<usize>(), 1);
    assert_eq!(w.cells.len(), 1);
}

#[tokio::test]
async fn bare_table_names_shared_across_databases_need_qualifying() {
    let world: Shared = Arc::default();
    let svc = service(&world);
    for name in ["First", "Second"] {
        svc.create_database(CreateDatabase {
            name: name.into(),
            owner_id: user(OWNER),
            acting_bot: None,
        })
        .await
        .unwrap();
    }

    let error = svc
        .query_sql(viewer(OWNER), "SELECT COUNT(*) FROM \"Table 1\"".into())
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            QueryError::Sql(ref message)
                if message == "table \"Table 1\" exists in First and Second — qualify it as First.Table 1 or Second.Table 1"
        ),
        "{error:?}"
    );

    let qualified = svc
        .query_sql(
            viewer(OWNER),
            "SELECT COUNT(*) FROM second.\"Table 1\"".into(),
        )
        .await
        .unwrap();
    assert_eq!(qualified.results[0].rows, vec![vec![SqlValue::Real(0.0)]]);
    assert_eq!(
        qualified.read_tables,
        vec![world.lock().unwrap().tables[1].id]
    );
}

#[tokio::test]
async fn a_scoped_statement_reaches_its_own_table_past_an_identically_named_database() {
    let world: Shared = Arc::default();
    let svc = service(&world);
    for _ in 0..2 {
        svc.create_database(CreateDatabase {
            name: "Untitled database".into(),
            owner_id: user(OWNER),
            acting_bot: None,
        })
        .await
        .unwrap();
    }
    let (first, second) = {
        let w = world.lock().unwrap();
        (w.databases[0].id, w.databases[1].id)
    };

    let unscoped = svc
        .query_sql(
            viewer(OWNER),
            "SELECT COUNT(*) FROM \"Untitled database\".\"Table 1\"".into(),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(unscoped, QueryError::Sql(ref message) if message.starts_with("table \"Table 1\" exists in")),
        "{unscoped:?}"
    );

    let scoped = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: Some(second),
                sql: "INSERT INTO \"Untitled database\".\"Table 1\" DEFAULT VALUES".into(),
                base_versions: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(scoped.inserted_row_ids.len(), 1);
    let w = world.lock().unwrap();
    let second_table = w
        .tables
        .iter()
        .find(|t| t.database_id == second)
        .unwrap()
        .id;
    let first_table = w.tables.iter().find(|t| t.database_id == first).unwrap().id;
    assert_eq!(w.rows.get(&second_table).map(Vec::len), Some(1));
    assert!(w.rows.get(&first_table).is_none_or(Vec::is_empty));
}

// ===== SQL: writes =====

#[tokio::test]
async fn insert_mints_rows_and_lands_cells_in_the_cell_store() {
    let seeded = seeded().await;
    let (world, svc, database_id, table_id, status_column) = (
        seeded.world,
        seeded.service,
        seeded.database_id,
        seeded.table_id,
        seeded.status_column,
    );
    let declined = world.lock().unwrap().definitions[&status_column.property_definition_id]
        .property_options
        .iter()
        .find(|option| option.value == PropertyOptionValue::String("Declined".into()))
        .unwrap()
        .id;
    let published_before = world.lock().unwrap().published.len();
    world.lock().unwrap().broker_events.clear();

    let outcome = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: "INSERT INTO guests (name, status, \"Plus ones\") VALUES ('Ada', 'Declined', 1), ('Bo', NULL, 0)".into(),
                base_versions: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(outcome.inserted_row_ids.len(), 2);
    assert_eq!(outcome.changes_applied, 2);
    assert!(outcome.results.is_empty());
    assert_eq!(
        outcome.new_versions,
        HashMap::from([(table_id, TableVersion(2))])
    );
    assert!(outcome.read_tables.is_empty());
    let (ada, bo) = (outcome.inserted_row_ids[0], outcome.inserted_row_ids[1]);
    let w = world.lock().unwrap();
    assert_eq!(
        w.rows[&table_id]
            .iter()
            .map(|row| row.id)
            .collect::<Vec<_>>(),
        vec![seeded.row_id, ada, bo]
    );
    assert_eq!(w.rows[&table_id][2].position, "0002");
    assert_eq!(
        w.cells[&ada],
        HashMap::from([
            (
                seeded.name_column.property_definition_id,
                PropertyValue::Str("Ada".into())
            ),
            (
                status_column.property_definition_id,
                PropertyValue::SelectOption(vec![declined])
            ),
            (
                seeded.plus_ones_column.property_definition_id,
                PropertyValue::Num(1.0)
            ),
        ])
    );
    assert_eq!(
        w.cells[&bo],
        HashMap::from([
            (
                seeded.name_column.property_definition_id,
                PropertyValue::Str("Bo".into())
            ),
            (
                seeded.plus_ones_column.property_definition_id,
                PropertyValue::Num(0.0)
            ),
        ]),
        "NULL is not a cell"
    );
    assert_eq!(w.tables[0].version, TableVersion(2));
    assert_eq!(w.published.len(), published_before + 1);
    assert_eq!(w.published.last(), Some(&(table_id, TableVersion(2))));
    assert_eq!(w.broker_events.len(), 1);
    assert_eq!(w.broker_events[0]["event_type"], "database.tables_changed");
    assert_eq!(
        w.broker_events[0]["metadata"]["database_id"],
        database_id.to_string()
    );
    assert_eq!(
        w.broker_events[0]["metadata"]["attribution"]["actor"],
        OWNER
    );
}

#[tokio::test]
async fn insert_default_values_mints_an_empty_row() {
    let seeded = seeded().await;
    let (world, svc, table_id) = (seeded.world, seeded.service, seeded.table_id);
    let outcome = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: "INSERT INTO guests DEFAULT VALUES".into(),
                base_versions: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(outcome.inserted_row_ids.len(), 1);
    assert_eq!(outcome.changes_applied, 1);
    let w = world.lock().unwrap();
    assert_eq!(w.rows[&table_id].len(), 2);
    assert!(!w.cells.contains_key(&outcome.inserted_row_ids[0]));
}

#[tokio::test]
async fn update_by_row_id_sets_and_clears_cells() {
    let seeded = seeded().await;
    let (world, svc, table_id, row_id) =
        (seeded.world, seeded.service, seeded.table_id, seeded.row_id);
    let declined = world.lock().unwrap().definitions[&seeded.status_column.property_definition_id]
        .property_options
        .iter()
        .find(|option| option.value == PropertyOptionValue::String("Declined".into()))
        .unwrap()
        .id;
    let published_before = world.lock().unwrap().published.len();

    let outcome = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: format!(
                    "UPDATE guests SET status = 'Declined', \"Plus ones\" = NULL WHERE row_id = '{row_id}'"
                ),
                base_versions: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(outcome.changes_applied, 1);
    assert!(outcome.inserted_row_ids.is_empty());
    assert_eq!(
        outcome.new_versions,
        HashMap::from([(table_id, TableVersion(2))])
    );
    {
        let w = world.lock().unwrap();
        assert_eq!(
            w.cells[&row_id],
            HashMap::from([
                (
                    seeded.name_column.property_definition_id,
                    PropertyValue::Str("Sam".into())
                ),
                (
                    seeded.status_column.property_definition_id,
                    PropertyValue::SelectOption(vec![declined])
                ),
            ])
        );
        assert_eq!(w.published.len(), published_before + 1);
    }

    let read = svc
        .query_sql(
            viewer(OWNER),
            "SELECT status, \"Plus ones\" FROM guests".into(),
        )
        .await
        .unwrap();
    assert_eq!(
        read.results[0].rows,
        vec![vec![
            SqlValue::Text(row_id.to_string()),
            SqlValue::Text("Declined".into()),
            SqlValue::Null,
        ]]
    );
}

#[tokio::test]
async fn delete_by_row_id_removes_the_row_and_its_cells() {
    let seeded = seeded().await;
    let (world, svc, table_id, row_id) =
        (seeded.world, seeded.service, seeded.table_id, seeded.row_id);
    let outcome = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: format!("DELETE FROM guests WHERE row_id = '{row_id}'"),
                base_versions: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(outcome.changes_applied, 1);
    assert_eq!(
        outcome.new_versions,
        HashMap::from([(table_id, TableVersion(2))])
    );
    {
        let w = world.lock().unwrap();
        assert!(w.rows[&table_id].is_empty());
        assert!(!w.cells.contains_key(&row_id));
    }
    let read = svc
        .query_sql(viewer(OWNER), "SELECT name FROM guests".into())
        .await
        .unwrap();
    assert_eq!(
        read.results[0]
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        vec!["row_id", "Name"]
    );
    assert!(read.results[0].rows.is_empty());

    // Deleting it again names a row the table no longer has.
    let error = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: format!("DELETE FROM guests WHERE row_id = '{row_id}'"),
                base_versions: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(error, QueryError::Sql(ref message) if *message == format!("row 1: no row {row_id} in this table")),
        "{error:?}"
    );
    assert_eq!(world.lock().unwrap().tables[0].version, TableVersion(2));
}

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
    svc.create_column(
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
    let sessions_version = world
        .lock()
        .unwrap()
        .tables
        .iter()
        .find(|table| table.id == sessions.id)
        .unwrap()
        .version;

    let error = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: format!("UPDATE sessions SET title = 'Hijacked' WHERE row_id = '{row_id}'"),
                base_versions: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(error, QueryError::Sql(ref message) if *message == format!("row 1: no row {row_id} in this table")),
        "{error:?}"
    );
    let error = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: format!("DELETE FROM sessions WHERE row_id = '{row_id}'"),
                base_versions: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(error, QueryError::Sql(ref message) if *message == format!("row 1: no row {row_id} in this table")),
        "{error:?}"
    );

    let w = world.lock().unwrap();
    assert_eq!(w.cells, cells_before);
    assert_eq!(w.rows[&table_id].len(), 1);
    assert_eq!(
        w.tables
            .iter()
            .find(|table| table.id == sessions.id)
            .unwrap()
            .version,
        sessions_version
    );
}

#[tokio::test]
async fn queries_refuse_writes_even_for_owners_without_changes_or_events() {
    let seeded = seeded().await;
    let (world, svc, table_id, row_id) =
        (seeded.world, seeded.service, seeded.table_id, seeded.row_id);
    let (cells, published, events, version) = {
        let world = world.lock().unwrap();
        (
            world.cells.clone(),
            world.published.len(),
            world.broker_events.len(),
            world.tables[0].version,
        )
    };
    for sql in [
        format!("UPDATE guests SET status = 'Declined' WHERE row_id = '{row_id}'"),
        format!("DELETE FROM guests WHERE row_id = '{row_id}'"),
        "INSERT INTO guests (name) VALUES ('Intruder')".to_string(),
        "INSERT INTO guests DEFAULT VALUES".to_string(),
    ] {
        let error = svc.query_sql(viewer(OWNER), sql.clone()).await.unwrap_err();
        assert!(
            matches!(error, QueryError::ReadOnly(ref message) if message == "queries cannot change data"),
            "{sql}: {error:?}"
        );
    }
    let world = world.lock().unwrap();
    assert_eq!(world.cells, cells);
    assert_eq!(world.published.len(), published);
    assert_eq!(world.broker_events.len(), events);
    assert_eq!(world.tables[0].version, version);
    assert_eq!(world.rows[&table_id].len(), 1);
}

#[tokio::test]
async fn view_grant_can_read_but_not_write() {
    let seeded = seeded().await;
    let (world, svc, table_id, row_id) =
        (seeded.world, seeded.service, seeded.table_id, seeded.row_id);
    let read = svc
        .exec_sql(
            viewer(VIEWER),
            ExecRequest {
                scope: None,
                sql: "SELECT COUNT(*) FROM guests".into(),
                base_versions: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(read.results[0].rows, vec![vec![SqlValue::Real(1.0)]]);

    for sql in [
        format!("DELETE FROM guests WHERE row_id = '{row_id}'"),
        format!("UPDATE guests SET name = 'Changed' WHERE row_id = '{row_id}'"),
        "INSERT INTO guests (name) VALUES ('Intruder')".to_string(),
    ] {
        let err = svc
            .exec_sql(
                viewer(VIEWER),
                ExecRequest {
                    scope: None,
                    sql: sql.clone(),
                    base_versions: None,
                },
            )
            .await
            .unwrap_err();
        assert!(
            matches!(err, QueryError::ReadOnly(ref message) if message == "table Guests is read-only"),
            "{sql}: {err:?}"
        );
    }
    {
        let w = world.lock().unwrap();
        assert_eq!(w.rows[&table_id].len(), 1);
        assert_eq!(w.tables[0].version, TableVersion(1));
    }

    // A stranger's catalog has no such table at all.
    let err = svc
        .exec_sql(
            viewer(STRANGER),
            ExecRequest {
                scope: None,
                sql: "SELECT * FROM guests".into(),
                base_versions: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, QueryError::Sql(ref message) if message == "unknown table guests"),
        "{err:?}"
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
    svc.create_column(
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

    // VIEWER writes their own database and only reads OWNER's.
    let inserted = svc
        .exec_sql(
            viewer(VIEWER),
            ExecRequest {
                scope: None,
                sql: "INSERT INTO rooms (name) VALUES ('Main Hall')".into(),
                base_versions: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        inserted.new_versions,
        HashMap::from([(rooms, TableVersion(1))])
    );
    let err = svc
        .exec_sql(
            viewer(VIEWER),
            ExecRequest {
                scope: None,
                sql: "INSERT INTO guests (name) VALUES ('Main Hall')".into(),
                base_versions: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, QueryError::ReadOnly(ref message) if message == "table Guests is read-only"),
        "{err:?}"
    );
    let both = svc
        .query_sql(viewer(VIEWER), "SELECT name FROM rooms".into())
        .await
        .unwrap();
    assert_eq!(
        both.results[0].rows[0][1],
        SqlValue::Text("Main Hall".into())
    );
    assert_eq!(both.read_database_ids, vec![venue.id]);

    // OWNER has no grant on Venue: the table does not exist for them.
    let err = svc
        .query_sql(viewer(OWNER), "SELECT * FROM rooms".into())
        .await
        .unwrap_err();
    assert!(
        matches!(err, QueryError::Sql(ref message) if message == "unknown table rooms"),
        "{err:?}"
    );
    let w = world.lock().unwrap();
    assert_eq!(w.rows[&rooms].len(), 1);
    assert_eq!(w.rows[&seeded.table_id].len(), 1);
}

#[tokio::test]
async fn has_predicate_runs_end_to_end() {
    let seeded = seeded().await;
    let (svc, database_id, table_id) = (seeded.service, seeded.database_id, seeded.table_id);
    svc.create_column(
        receipt::<EditAccessLevel>(database_id, OWNER, AccessLevel::Owner),
        viewer(OWNER),
        CreateColumn {
            infer_type: false,
            table_id,
            binding: ColumnBinding::NewDefinition {
                name: "Tags".into(),
                data_type: DataType::Tag,
                is_multi_select: true,
                options: vec!["vip".into()],
            },
            config: None,
        },
    )
    .await
    .unwrap();
    svc.exec_sql(
        viewer(OWNER),
        ExecRequest {
            scope: None,
            sql: "INSERT INTO guests (name, tags) VALUES ('Tara', ['vip']), ('Uma', NULL)".into(),
            base_versions: None,
        },
    )
    .await
    .unwrap();

    let vip = svc
        .query_sql(
            viewer(OWNER),
            "SELECT name FROM guests WHERE tags HAS 'vip' ORDER BY name".into(),
        )
        .await
        .unwrap();
    assert_eq!(
        vip.results[0]
            .rows
            .iter()
            .map(|row| row[1].clone())
            .collect::<Vec<_>>(),
        vec![SqlValue::Text("Tara".into())]
    );
    assert_eq!(vip.read_tables, vec![table_id]);

    let not_vip = svc
        .query_sql(
            viewer(OWNER),
            "SELECT name FROM guests WHERE tags NOT HAS 'vip' ORDER BY name".into(),
        )
        .await
        .unwrap();
    assert_eq!(
        not_vip.results[0]
            .rows
            .iter()
            .map(|row| row[1].clone())
            .collect::<Vec<_>>(),
        vec![SqlValue::Text("Sam".into()), SqlValue::Text("Uma".into())]
    );

    let err = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: "INSERT INTO guests (name, tags) VALUES ('Vic', ['nope'])".into(),
                base_versions: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, QueryError::Sql(ref message) if message == "\"nope\" is not an option of \"Tags\" (vip)"),
        "{err:?}"
    );
}

// ===== Select options are explicit schema =====

#[tokio::test]
async fn a_select_column_with_no_options_accepts_nothing() {
    let seeded = seeded().await;
    let (svc, database_id, table_id) = (seeded.service, seeded.database_id, seeded.table_id);
    svc.create_column(
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

    let err = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: "INSERT INTO guests (name, stage) VALUES ('Ada', 'Main')".into(),
                base_versions: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, QueryError::Sql(ref message) if message == "\"Main\" is not an option of \"Stage\" ()"),
        "{err:?}"
    );
}

/// The point of the operation: the write that failed succeeds once the option
/// exists, and the table's version moves because its schema did.
#[tokio::test]
async fn add_column_options_extends_what_sql_accepts_and_bumps_the_version() {
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
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: "INSERT INTO guests (name, status) VALUES ('Bo', 'Waitlisted')".into(),
                base_versions: None,
            },
        )
        .await
        .expect("the option now resolves");
    let read = svc
        .query_sql(
            viewer(OWNER),
            "SELECT status FROM guests WHERE name = 'Bo'".into(),
        )
        .await
        .unwrap();
    assert_eq!(
        read.results[0].rows,
        vec![vec![
            SqlValue::Text(inserted.inserted_row_ids[0].to_string()),
            SqlValue::Text("Waitlisted".into()),
        ]]
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
        matches!(err, DatabaseError::InvalidSchemaOperation(ref m) if m.contains("select")),
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
        matches!(err, DatabaseError::InvalidSchemaOperation(ref m) if m.contains("options")),
        "{err:?}"
    );
}

/// A numeric select stores numbers, so its labels have to be numbers — and
/// the label SQL sees is the normalized one.
#[tokio::test]
async fn numeric_select_options_are_parsed_as_numbers() {
    let seeded = seeded().await;
    let (svc, db, guests) = (seeded.service, seeded.database_id, seeded.table_id);

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
        matches!(err, DatabaseError::InvalidSchemaOperation(ref m) if m.contains("not a number")),
        "{err:?}"
    );

    svc.create_column(
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
    let inserted = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: "INSERT INTO guests (name, priority) VALUES ('Ada', '2')".into(),
                base_versions: None,
            },
        )
        .await
        .expect("`2.0` and `2` are one option, written as `2`");
    let read = svc
        .query_sql(
            viewer(OWNER),
            "SELECT priority FROM guests WHERE name = 'Ada'".into(),
        )
        .await
        .unwrap();
    assert_eq!(
        read.results[0].rows,
        vec![vec![
            SqlValue::Text(inserted.inserted_row_ids[0].to_string()),
            SqlValue::Text("2".into()),
        ]]
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
            matches!(err, DatabaseError::InvalidSchemaOperation(ref m) if m.contains("empty")),
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
        matches!(err, DatabaseError::InvalidSchemaOperation(ref m) if m.contains("at most")),
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
    svc.exec_sql(
        viewer(OWNER),
        ExecRequest {
            scope: None,
            sql: format!("UPDATE guests SET status = 'Declined' WHERE row_id = '{row_id}'"),
            base_versions: None,
        },
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
    svc.exec_sql(
        Viewer {
            user_id: user(OWNER),
            acting_bot: Some(agent),
        },
        ExecRequest {
            scope: None,
            sql: format!("UPDATE guests SET status = 'Declined' WHERE row_id = '{row_id}'"),
            base_versions: None,
        },
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
