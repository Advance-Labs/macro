//! In-memory fakes for every port, over one shared world the tests inspect.

mod apply_writes;

use models_databases::position::{key_between, keys_between};

use super::*;
use apply_writes::apply_in_world;

#[derive(Debug, thiserror::Error)]
#[error("fake failure")]
pub(super) struct FakeError;

/// Shared mutable world the fakes read and write.
#[derive(Default)]
pub(super) struct World {
    pub(super) databases: Vec<Database>,
    pub(super) tables: Vec<Table>,
    pub(super) columns: Vec<Column>,
    pub(super) definitions: HashMap<PropertyDefinitionId, PropertyDefinitionWithOptions>,
    /// Row identities per table, in position order.
    pub(super) rows: HashMap<TableId, Vec<RowRef>>,
    /// The cell store: a row's cells keyed by definition.
    pub(super) cells: HashMap<RowId, HashMap<Uuid, PropertyValue>>,
    pub(super) grants: HashMap<String, Vec<(DatabaseId, AccessLevel)>>,
    pub(super) published: Vec<(TableId, TableVersion)>,
    /// Every awareness relay the service asked for.
    pub(super) awareness: Vec<(DatabaseId, String, Awareness)>,
    /// Simulate the gateway refusing an awareness relay.
    pub(super) awareness_relay_fails: bool,
    pub(super) share_updates: Vec<Vec<models_permissions::share_permission::channel_share_permission::UpdateChannelSharePermission>>,
    /// Every `macro.databases` envelope the service handed the broker.
    pub(super) broker_events: Vec<serde_json::Value>,
    /// Every `settle_inference` call, newest last.
    pub(super) settled: Vec<(TableId, Vec<PropertyDefinitionId>)>,
    /// Simulate a parent removed between domain validation and the write.
    pub(super) table_write_not_found: bool,
    /// Saved queries, oldest first.
    pub(super) queries: Vec<SavedQuery>,
    /// How many write batches the cell store was handed.
    pub(super) write_batches: usize,
    /// The shared definitions each user may change, as the properties system
    /// answers it.
    pub(super) editable_definitions: HashMap<String, Vec<PropertyDefinitionId>>,
    /// Every table's views.
    pub(super) views: Vec<DatabaseView>,
    /// Where each board's cards sit.
    pub(super) positions: HashMap<ViewId, Vec<CardPosition>>,
}

/// Store views a schema change rewrote; `false` when one is gone.
pub(super) fn rewrite_views(world: &mut World, views: &[DatabaseView]) -> bool {
    for view in views {
        let Some(stored) = world.views.iter_mut().find(|stored| stored.id == view.id) else {
            return false;
        };
        *stored = view.clone();
    }
    true
}

pub(super) type Shared = Arc<Mutex<World>>;

#[derive(Clone)]
pub(super) struct FakeRepo(pub(super) Shared);
#[derive(Clone)]
pub(super) struct FakeDefinitions(pub(super) Shared);
#[derive(Clone)]
pub(super) struct FakeCells(pub(super) Shared);
#[derive(Clone)]
pub(super) struct FakeEvents(pub(super) Shared);
#[derive(Clone)]
pub(super) struct FakeAccess(pub(super) Shared);

impl DatabasesRepo for FakeRepo {
    type Error = FakeError;
    async fn create_database(
        &self,
        command: &CreateDatabase,
        first_table: FirstTable,
    ) -> Result<Database, FakeError> {
        let database = Database {
            id: Uuid::new_v4(),
            name: command.name.clone(),
            owner_id: command.owner_id.as_ref().to_string(),
            created_at: Utc::now(),
            trashed_at: None,
        };
        let title = definition(
            first_table.title_column,
            DataType::String,
            false,
            PropertyOwner::Database {
                database_id: database.id,
            },
        );
        let table = Table {
            id: Uuid::new_v4(),
            database_id: database.id,
            name: first_table.name.to_string(),
            position: key_between(None, None).unwrap(),
            version: TableVersion(0),
        };
        let mut world = self.0.lock().unwrap();
        world.databases.push(database.clone());
        world.columns.push(Column {
            id: Uuid::new_v4(),
            table_id: table.id,
            property_definition_id: title.definition.id,
            display_name: None,
            position: key_between(None, None).unwrap(),
            infer_type: false,
            config: None,
        });
        world.definitions.insert(title.definition.id, title);
        world.tables.push(table);
        world
            .grants
            .entry(command.owner_id.as_ref().to_string())
            .or_default()
            .push((database.id, AccessLevel::Owner));
        Ok(database)
    }
    async fn get_database(
        &self,
        id: DatabaseId,
    ) -> Result<Option<(Database, Vec<Table>)>, FakeError> {
        let world = self.0.lock().unwrap();
        Ok(world
            .databases
            .iter()
            .find(|database| database.id == id)
            .map(|database| {
                (
                    database.clone(),
                    world
                        .tables
                        .iter()
                        .filter(|table| table.database_id == id)
                        .cloned()
                        .collect(),
                )
            }))
    }
    async fn rename_database(&self, id: DatabaseId, name: &str) -> Result<bool, FakeError> {
        let mut world = self.0.lock().unwrap();
        if let Some(database) = world
            .databases
            .iter_mut()
            .find(|database| database.id == id)
        {
            database.name = name.to_string();
            Ok(true)
        } else {
            Ok(false)
        }
    }
    async fn trash_database(
        &self,
        id: DatabaseId,
        trashed_at: chrono::DateTime<Utc>,
    ) -> Result<bool, FakeError> {
        let mut world = self.0.lock().unwrap();
        if let Some(database) = world
            .databases
            .iter_mut()
            .find(|database| database.id == id)
        {
            database.trashed_at = Some(trashed_at);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    async fn restore_database(&self, id: DatabaseId) -> Result<bool, FakeError> {
        let mut world = self.0.lock().unwrap();
        if let Some(database) = world
            .databases
            .iter_mut()
            .find(|database| database.id == id)
        {
            database.trashed_at = None;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    async fn delete_database(&self, id: DatabaseId) -> Result<(), FakeError> {
        let mut world = self.0.lock().unwrap();
        world.databases.retain(|database| database.id != id);
        let table_ids: Vec<TableId> = world
            .tables
            .iter()
            .filter(|table| table.database_id == id)
            .map(|table| table.id)
            .collect();
        world.tables.retain(|table| table.database_id != id);
        world
            .columns
            .retain(|column| !table_ids.contains(&column.table_id));
        let row_ids: Vec<RowId> = table_ids
            .iter()
            .filter_map(|table_id| world.rows.remove(table_id))
            .flatten()
            .map(|row| row.id)
            .collect();
        world.cells.retain(|row_id, _| !row_ids.contains(row_id));
        // The Postgres adapter purges `entity_access` rows in the same
        // transaction; the fake's grant map stands in for that table.
        for grants in world.grants.values_mut() {
            grants.retain(|(database_id, _)| *database_id != id);
        }
        Ok(())
    }
    async fn create_table(&self, command: &CreateTable) -> Result<TableMutationOutcome, FakeError> {
        let mut world = self.0.lock().unwrap();
        if world.table_write_not_found {
            return Ok(TableMutationOutcome::NotFound);
        }
        let siblings = || {
            world
                .tables
                .iter()
                .filter(|table| table.database_id == command.database_id)
        };
        if siblings().any(|table| same_name(&table.name, &command.name)) {
            return Ok(TableMutationOutcome::Conflict);
        }
        let last = siblings().map(|table| table.position.as_str()).max();
        let table = Table {
            id: Uuid::new_v4(),
            database_id: command.database_id,
            name: command.name.clone(),
            position: key_between(last, None).unwrap(),
            version: TableVersion(0),
        };
        world.tables.push(table.clone());
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
        for (id, position) in ids.iter().zip(keys_between(None, None, ids.len()).unwrap()) {
            let table = world
                .tables
                .iter_mut()
                .find(|table| table.id == *id)
                .unwrap();
            table.position = position;
            table.version.0 += 1;
        }
        world
            .tables
            .sort_by(|left, right| left.position.cmp(&right.position));
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
        let mut world = self.0.lock().unwrap();
        if world.table_write_not_found
            || !world
                .databases
                .iter()
                .any(|database| database.id == table.database_id && database.trashed_at.is_none())
            || !world.tables.iter().any(|stored| stored.id == table.id)
        {
            return Ok(TableDeletion::NotFound);
        }
        if world
            .tables
            .iter()
            .filter(|stored| stored.database_id == table.database_id)
            .count()
            <= 1
        {
            return Ok(TableDeletion::LastTable);
        }
        world.tables.retain(|stored| stored.id != table.id);
        world.columns.retain(|column| column.table_id != table.id);
        // The schema's cleanup triggers take the rows' cells with them.
        for row in world.rows.remove(&table.id).unwrap_or_default() {
            world.cells.remove(&row.id);
        }
        Ok(TableDeletion::Deleted)
    }
    async fn create_column(
        &self,
        table_id: TableId,
        property_definition_id: PropertyDefinitionId,
        command: &CreateColumn,
    ) -> Result<(ColumnId, TableVersion), FakeError> {
        let mut world = self.0.lock().unwrap();
        let last = world
            .columns
            .iter()
            .filter(|column| column.table_id == table_id)
            .map(|column| column.position.as_str())
            .max();
        let column = Column {
            infer_type: command.infer_type,
            display_name: None,
            id: Uuid::new_v4(),
            table_id,
            property_definition_id,
            position: key_between(last, None).unwrap(),
            config: command.config.clone(),
        };
        world.columns.push(column.clone());
        // Unlike Postgres the fake leaves the version alone, so the seeded
        // tests' versions count only data writes.
        let version = world
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
        let mut world = self.0.lock().unwrap();
        let has_value = world.rows.get(&table.id).is_some_and(|rows| {
            rows.iter().any(|row| {
                world
                    .cells
                    .get(&row.id)
                    .is_some_and(|cells| cells.contains_key(&column.property_definition_id))
            })
        });
        if has_value {
            return Ok(None);
        }
        let Some(table_index) = world
            .tables
            .iter()
            .position(|stored| stored.id == table.id && stored.version == table.version)
        else {
            return Ok(None);
        };
        let Some(column_index) = world.columns.iter().position(|stored| {
            stored.id == column.id
                && stored.table_id == table.id
                && stored.property_definition_id == column.property_definition_id
                && stored.infer_type
        }) else {
            return Ok(None);
        };
        world.columns[column_index].property_definition_id = definition_id;
        world.columns[column_index].infer_type = false;
        world.tables[table_index].version.0 += 1;
        Ok(Some(world.tables[table_index].version))
    }
    async fn delete_column(
        &self,
        table: &Table,
        column: &Column,
        views: &[DatabaseView],
    ) -> Result<Option<ColumnSchemaOutcome>, FakeError> {
        let mut world = self.0.lock().unwrap();
        let Some(table_index) = world
            .tables
            .iter()
            .position(|stored| stored.id == table.id && stored.version == table.version)
        else {
            return Ok(None);
        };
        let Some(column_index) = world
            .columns
            .iter()
            .position(|stored| stored.id == column.id && stored.table_id == table.id)
        else {
            return Ok(None);
        };
        if !rewrite_views(&mut world, views) {
            return Ok(None);
        }
        let removed = world.columns.remove(column_index);
        let rows: Vec<RowId> = world
            .rows
            .get(&table.id)
            .map(|rows| rows.iter().map(|row| row.id).collect())
            .unwrap_or_default();
        for row in rows {
            if let Some(cells) = world.cells.get_mut(&row) {
                cells.remove(&removed.property_definition_id);
            }
        }
        world.tables[table_index].version.0 += 1;
        Ok(Some(ColumnSchemaOutcome {
            table_versions: HashMap::from([(table.id, world.tables[table_index].version)]),
        }))
    }
    async fn reorder_columns(
        &self,
        table: &Table,
        ids: &[ColumnId],
    ) -> Result<Option<TableVersion>, FakeError> {
        let mut world = self.0.lock().unwrap();
        let Some(table_index) = world
            .tables
            .iter()
            .position(|stored| stored.id == table.id && stored.version == table.version)
        else {
            return Ok(None);
        };
        for (id, position) in ids.iter().zip(keys_between(None, None, ids.len()).unwrap()) {
            let Some(column) = world
                .columns
                .iter_mut()
                .find(|column| column.id == *id && column.table_id == table.id)
            else {
                return Ok(None);
            };
            column.position = position;
        }
        world.tables[table_index].version.0 += 1;
        Ok(Some(world.tables[table_index].version))
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
        let world = self.0.lock().unwrap();
        Ok(world
            .tables
            .iter()
            .filter(|table| table_ids.contains(&table.id))
            .map(|table| (table.id, table.version))
            .collect())
    }
    async fn databases_by_ids(&self, ids: &[DatabaseId]) -> Result<Vec<Database>, FakeError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .databases
            .iter()
            .filter(|database| ids.contains(&database.id))
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
            .filter(|table| database_ids.contains(&table.database_id))
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
            .filter(|column| table_ids.contains(&column.table_id))
            .cloned()
            .collect();
        columns.sort_by(|left, right| {
            (left.table_id, &left.position).cmp(&(right.table_id, &right.position))
        });
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
        views.sort_by(|left, right| {
            (left.table_id, &left.position).cmp(&(right.table_id, &right.position))
        });
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
        let world = self.0.lock().unwrap();
        Ok(rows
            .iter()
            .filter_map(|row| world.cells.get(row).map(|cells| (*row, cells.clone())))
            .collect())
    }
    async fn column_cells(
        &self,
        rows: &[RowId],
        definition: PropertyDefinitionId,
    ) -> Result<HashMap<RowId, PropertyValue>, FakeError> {
        let world = self.0.lock().unwrap();
        Ok(rows
            .iter()
            .filter_map(|row| {
                let value = world.cells.get(row)?.get(&definition)?;
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
        let mut world = self.0.lock().unwrap();
        let Some(table_index) = world
            .tables
            .iter()
            .position(|stored| stored.id == table.id && stored.version == table.version)
        else {
            return Ok(None);
        };
        let Some(column_index) = world.columns.iter().position(|stored| {
            stored.id == replacement.column.id
                && stored.table_id == table.id
                && stored.property_definition_id == replacement.column.property_definition_id
        }) else {
            return Ok(None);
        };
        if !rewrite_views(&mut world, views) {
            return Ok(None);
        }
        for (row, value) in &replacement.values {
            world
                .cells
                .entry(*row)
                .or_default()
                .insert(replacement.definition_id, value.clone());
        }
        let column = &mut world.columns[column_index];
        column.property_definition_id = replacement.definition_id;
        column.config = replacement.config.clone();
        column.infer_type = false;
        world.tables[table_index].version.0 += 1;
        Ok(Some(world.tables[table_index].version))
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
        let mut world = self.0.lock().unwrap();
        world.write_batches += 1;
        let before = (
            world.tables.clone(),
            world.columns.clone(),
            world.definitions.clone(),
            world.rows.clone(),
            world.cells.clone(),
            world.settled.clone(),
            world.views.clone(),
            world.positions.clone(),
        );
        let outcome = apply_in_world(&mut world, writes);
        if !matches!(outcome, Ok(WritesOutcome::Applied { .. })) {
            (
                world.tables,
                world.columns,
                world.definitions,
                world.rows,
                world.cells,
                world.settled,
                world.views,
                world.positions,
            ) = before;
        }
        outcome
    }
}

impl ColumnDefinitionStore for FakeDefinitions {
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
                let created = definition(
                    name,
                    *data_type,
                    *is_multi_select,
                    PropertyOwner::Database { database_id },
                );
                let id = created.definition.id;
                self.0.lock().unwrap().definitions.insert(id, created);
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
        let mut created = definition(
            name,
            data_type,
            is_multi_select,
            PropertyOwner::Database { database_id },
        );
        created.definition.specific_entity_type = specific_entity_type;
        self.0
            .lock()
            .unwrap()
            .definitions
            .insert(created.definition.id, created.clone());
        Ok(created)
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
        let mut world = self.0.lock().unwrap();
        let extended = world.definitions.get_mut(&definition_id).ok_or(FakeError)?;
        let mut display_order = extended
            .property_options
            .iter()
            .map(|option| option.display_order)
            .max()
            .map_or(0, |highest| highest + 1);
        for value in values {
            extended.property_options.push(PropertyOption {
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
        Ok(extended.property_options.clone())
    }
    async fn definitions(
        &self,
        ids: &[PropertyDefinitionId],
    ) -> Result<Vec<PropertyDefinitionWithOptions>, FakeError> {
        let world = self.0.lock().unwrap();
        Ok(ids
            .iter()
            .filter_map(|id| world.definitions.get(id).cloned())
            .collect())
    }
    async fn editable_definitions(
        &self,
        viewer: &Viewer,
        ids: &[PropertyDefinitionId],
    ) -> Result<Vec<PropertyDefinitionId>, FakeError> {
        let world = self.0.lock().unwrap();
        let editable = world
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
        let mut world = self.0.lock().unwrap();
        if world.awareness_relay_fails {
            return Err(FakeError);
        }
        world
            .awareness
            .push((database_id, user_id.as_ref().to_string(), state.clone()));
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
pub(super) struct RecordingBroker(pub(super) Shared);

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
