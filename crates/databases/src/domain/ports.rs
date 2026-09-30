//! Ports: the contracts between the domain service and its adapters.
//!
//! Persistence is split by ownership. [`DatabasesRepo`] owns the databases,
//! tables, column placements and row identities (`database_rows`: id,
//! table, position). [`CellStore`] owns nothing: it is the domain's view of
//! the properties system, where a row's cells live as entity properties of
//! the `DATABASE_ROW` entity that the row id names. [`ColumnDefinitionStore`]
//! is the same boundary for the definitions behind columns.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use entity_access::domain::models::{
    AccessLevel, EditAccessLevel, EntityAccessReceipt, OwnerAccessLevel, ViewAccessLevel,
};
use models_properties::service::property_definition_with_options::PropertyDefinitionWithOptions;
use models_properties::service::property_option::{PropertyOption, PropertyOptionValue};
use models_properties::service::property_value::PropertyValue;

use crate::domain::models::{
    AddColumnOptions, Awareness, Column, ColumnBinding, ColumnDetail, ColumnId, CreateColumn,
    CreateDatabase, CreateTable, Database, DatabaseDetail, DatabaseError, DatabaseId, ExecOutcome,
    ExecRequest, InferColumnType, InferColumnTypeOutcome, ListedDatabase, PropertyDefinitionId,
    QueryError, RenameColumnOutcome, RowId, RowRef, Table, TableId, TableMutationOutcome,
    TableVersion, Viewer,
};
use crate::domain::models::{ChangeColumnType, ColumnReplacement, ColumnSchemaOutcome};
use crate::domain::models::{
    QueryDefinition, QueryId, SavedQuery, TableDeletion, TableOrderOutcome,
};

/// Persistence for databases, tables, column placements and row identities.
pub trait DatabasesRepo: Send + Sync + 'static {
    /// The error type returned by repository operations.
    type Err: std::error::Error + Send + Sync + 'static;

    /// Create a database with its starter table, granting the creator owner
    /// access.
    fn create_database(
        &self,
        cmd: &CreateDatabase,
        starter_table_name: &str,
    ) -> impl Future<Output = Result<Database, Self::Err>> + Send;

    /// A database and its tables, if it exists.
    fn get_database(
        &self,
        id: DatabaseId,
    ) -> impl Future<Output = Result<Option<(Database, Vec<Table>)>, Self::Err>> + Send;

    /// Rename a database.
    fn rename_database(
        &self,
        id: DatabaseId,
        name: &str,
    ) -> impl Future<Output = Result<(), Self::Err>> + Send;

    /// Move a database to the trash.
    fn trash_database(
        &self,
        id: DatabaseId,
        trashed_at: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), Self::Err>> + Send;

    /// Restore a trashed database.
    fn restore_database(
        &self,
        id: DatabaseId,
    ) -> impl Future<Output = Result<(), Self::Err>> + Send;

    /// Remove a database permanently.
    fn delete_database(&self, id: DatabaseId)
    -> impl Future<Output = Result<(), Self::Err>> + Send;

    /// Add a table to a database.
    fn create_table(
        &self,
        cmd: &CreateTable,
    ) -> impl Future<Output = Result<TableMutationOutcome, Self::Err>> + Send;

    /// Rename a table, guarded by its previous name.
    fn rename_table(
        &self,
        table: &Table,
        name: &str,
        previous_name: &str,
    ) -> impl Future<Output = Result<TableMutationOutcome, Self::Err>> + Send;

    /// Give a database's tables the positions of `table_ids`, which must name
    /// every one of its tables exactly once.
    fn reorder_tables(
        &self,
        database_id: DatabaseId,
        table_ids: &[TableId],
    ) -> impl Future<Output = Result<TableOrderOutcome, Self::Err>> + Send;

    /// Remove a table with its columns and row identities, unless it is its
    /// database's last one.
    fn delete_table(
        &self,
        table: &Table,
    ) -> impl Future<Output = Result<TableDeletion, Self::Err>> + Send;

    /// Bind a definition into a table as a new column placement.
    fn create_column(
        &self,
        table_id: TableId,
        property_definition_id: PropertyDefinitionId,
        cmd: &CreateColumn,
    ) -> impl Future<Output = Result<ColumnId, Self::Err>> + Send;

    /// Rename a column placement.
    fn rename_column(
        &self,
        table: &Table,
        column: &Column,
        name: &str,
    ) -> impl Future<Output = Result<Option<RenameColumnOutcome>, Self::Err>> + Send;

    /// Settle an untyped column on a definition, provided no row has a value
    /// in it yet.
    fn infer_column_type(
        &self,
        table: &Table,
        column: &Column,
        definition_id: PropertyDefinitionId,
    ) -> impl Future<Output = Result<Option<TableVersion>, Self::Err>> + Send;

    /// Swap a placement onto a fresh definition. The converted cells in the
    /// replacement are the service's to write afterwards.
    fn replace_column(
        &self,
        table: &Table,
        replacement: &ColumnReplacement,
    ) -> impl Future<Output = Result<Option<TableVersion>, Self::Err>> + Send;

    /// Remove a column placement.
    fn delete_column(
        &self,
        table: &Table,
        column: &Column,
    ) -> impl Future<Output = Result<Option<ColumnSchemaOutcome>, Self::Err>> + Send;

    /// Reorder a table's column placements.
    fn reorder_columns(
        &self,
        table: &Table,
        column_ids: &[ColumnId],
    ) -> impl Future<Output = Result<Option<TableVersion>, Self::Err>> + Send;

    /// Bump a table's version, answering the new one.
    fn bump_table_version(
        &self,
        table_id: TableId,
    ) -> impl Future<Output = Result<TableVersion, Self::Err>> + Send;

    /// Every row of a table, in position order.
    fn row_refs(
        &self,
        table_id: TableId,
    ) -> impl Future<Output = Result<Vec<RowRef>, Self::Err>> + Send;

    /// Append `count` empty rows to a table, answering them in order. `None`
    /// when the table is gone or its database is trashed.
    fn insert_rows(
        &self,
        table_id: TableId,
        created_by: &str,
        count: usize,
    ) -> impl Future<Output = Result<Option<Vec<RowRef>>, Self::Err>> + Send;

    /// Remove one row of a table; `false` if it was not there.
    fn delete_row(
        &self,
        table_id: TableId,
        row_id: RowId,
    ) -> impl Future<Output = Result<bool, Self::Err>> + Send;

    /// The table a row belongs to, if the row exists.
    fn row_table(
        &self,
        row_id: RowId,
    ) -> impl Future<Output = Result<Option<TableId>, Self::Err>> + Send;

    /// A first value landed in these columns: they no longer infer their
    /// type from it.
    fn settle_inference(
        &self,
        table_id: TableId,
        definitions: &[PropertyDefinitionId],
    ) -> impl Future<Output = Result<(), Self::Err>> + Send;

    /// Current versions for a set of tables.
    fn table_versions(
        &self,
        table_ids: &[TableId],
    ) -> impl Future<Output = Result<HashMap<TableId, TableVersion>, Self::Err>> + Send;

    /// Databases by id (missing ids are skipped).
    fn databases_by_ids(
        &self,
        ids: &[DatabaseId],
    ) -> impl Future<Output = Result<Vec<Database>, Self::Err>> + Send;

    /// Every table of the given databases, ordered by database then position.
    fn tables_for_databases(
        &self,
        database_ids: &[DatabaseId],
    ) -> impl Future<Output = Result<Vec<Table>, Self::Err>> + Send;

    /// Every column placement of the given tables, ordered by table then position.
    fn columns_for_tables(
        &self,
        table_ids: &[TableId],
    ) -> impl Future<Output = Result<Vec<Column>, Self::Err>> + Send;

    /// Store a new, immutable query.
    fn save_query(
        &self,
        database_id: Option<DatabaseId>,
        definition: &QueryDefinition,
        created_by: &str,
    ) -> impl Future<Output = Result<SavedQuery, Self::Err>> + Send;

    /// A saved query, if it exists.
    fn get_query(
        &self,
        id: QueryId,
    ) -> impl Future<Output = Result<Option<SavedQuery>, Self::Err>> + Send;
}

/// A row's cells, kept by the properties system as entity properties of the
/// `DATABASE_ROW` entity the row id names. Authorization is the domain
/// service's, resolved through the row's database; the store trusts its
/// caller.
pub trait CellStore: Send + Sync + 'static {
    /// The error type returned by the store.
    type Err: std::error::Error + Send + Sync + 'static;

    /// The cells of these rows, keyed by row then definition. A row with no
    /// cells is absent from the map.
    fn cells(
        &self,
        rows: &[RowId],
    ) -> impl Future<
        Output = Result<HashMap<RowId, HashMap<PropertyDefinitionId, PropertyValue>>, Self::Err>,
    > + Send;

    /// Set (or, with `None`, clear) cells on one row.
    fn write(
        &self,
        row: RowId,
        cells: &[(PropertyDefinitionId, Option<PropertyValue>)],
    ) -> impl Future<Output = Result<(), Self::Err>> + Send;

    /// Remove every cell of a row.
    fn clear(&self, row: RowId) -> impl Future<Output = Result<(), Self::Err>> + Send;
}

/// Which databases a viewer can reach, as `entity_access` answers it. The
/// domain treats it as the authorization boundary for SQL (the catalog is
/// built from it). Trash is not its concern: a trashed database's grants are
/// still answered, and the service drops them.
pub trait AccessDirectory: Send + Sync + 'static {
    /// The error type returned by directory operations.
    type Err: std::error::Error + Send + Sync + 'static;

    /// Every database the viewer holds a grant on, at the highest level.
    fn accessible_databases(
        &self,
        viewer: &Viewer,
    ) -> impl Future<Output = Result<Vec<(DatabaseId, AccessLevel)>, Self::Err>> + Send;

    /// The viewer's highest level on one database; `None` without a grant.
    fn database_access(
        &self,
        viewer: &Viewer,
        database_id: DatabaseId,
    ) -> impl Future<Output = Result<Option<AccessLevel>, Self::Err>> + Send;
}

/// The definitions behind columns: creating database-owned ones, binding
/// existing ones, and reading them back with their options.
pub trait ColumnDefinitionStore: Send + Sync + 'static {
    /// The error type returned by the store.
    type Err: std::error::Error + Send + Sync + 'static;

    /// The definition a column binding names, checked against the viewer.
    fn resolve_binding(
        &self,
        database_id: DatabaseId,
        viewer: &Viewer,
        binding: &ColumnBinding,
    ) -> impl Future<Output = Result<PropertyDefinitionId, Self::Err>> + Send;

    /// Create a definition owned by the database.
    fn create_typed_definition(
        &self,
        database_id: DatabaseId,
        name: &str,
        data_type: models_properties::DataType,
        is_multi_select: bool,
        specific_entity_type: Option<models_properties::EntityType>,
    ) -> impl Future<Output = Result<PropertyDefinitionWithOptions, Self::Err>> + Send;

    /// Remove a definition that no column binds any more.
    fn delete_unused_definition(
        &self,
        id: PropertyDefinitionId,
    ) -> impl Future<Output = Result<(), Self::Err>> + Send;

    /// Add options to a select definition, answering every option it has.
    fn add_options(
        &self,
        definition_id: PropertyDefinitionId,
        values: &[PropertyOptionValue],
    ) -> impl Future<Output = Result<Vec<PropertyOption>, Self::Err>> + Send;

    /// Definitions by id, with their options.
    fn definitions(
        &self,
        ids: &[PropertyDefinitionId],
    ) -> impl Future<Output = Result<Vec<PropertyDefinitionWithOptions>, Self::Err>> + Send;
}

/// Liveness: tell open clients a table changed.
pub trait TableEventPublisher: Send + Sync + 'static {
    /// The error type returned by the publisher.
    type Err: std::error::Error + Send + Sync + 'static;

    /// Announce a table's new version.
    fn table_changed(
        &self,
        database_id: DatabaseId,
        table_id: TableId,
        version: TableVersion,
    ) -> impl Future<Output = Result<(), Self::Err>> + Send;

    /// Relay where a viewer is to the database's other viewers.
    fn awareness(
        &self,
        database_id: DatabaseId,
        user_id: &str,
        state: &Awareness,
    ) -> impl Future<Output = Result<(), Self::Err>> + Send;
}

/// The databases domain service.
pub trait DatabasesService: Send + Sync + 'static {
    /// Create a database with a starter table.
    fn create_database(
        &self,
        cmd: CreateDatabase,
    ) -> impl Future<Output = Result<Database, DatabaseError>> + Send;

    /// Every database the viewer can see.
    fn list_databases(
        &self,
        viewer: Viewer,
    ) -> impl Future<Output = Result<Vec<ListedDatabase>, DatabaseError>> + Send;

    /// A database with its tables and columns.
    fn get_database(
        &self,
        receipt: EntityAccessReceipt<ViewAccessLevel>,
        viewer: Viewer,
    ) -> impl Future<Output = Result<DatabaseDetail, DatabaseError>> + Send;

    /// Rename a database.
    fn rename_database(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        name: String,
    ) -> impl Future<Output = Result<Database, DatabaseError>> + Send;

    /// Move a database to the trash.
    fn trash_database(
        &self,
        receipt: EntityAccessReceipt<OwnerAccessLevel>,
    ) -> impl Future<Output = Result<(), DatabaseError>> + Send;

    /// Restore a trashed database.
    fn restore_database(
        &self,
        receipt: EntityAccessReceipt<OwnerAccessLevel>,
    ) -> impl Future<Output = Result<(), DatabaseError>> + Send;

    /// Remove a database permanently.
    fn delete_database_permanently(
        &self,
        receipt: EntityAccessReceipt<OwnerAccessLevel>,
    ) -> impl Future<Output = Result<(), DatabaseError>> + Send;

    /// Add a table.
    fn create_table(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        cmd: CreateTable,
    ) -> impl Future<Output = Result<Table, DatabaseError>> + Send;

    /// Rename a table.
    fn rename_table(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        table_id: TableId,
        name: String,
        previous_name: String,
    ) -> impl Future<Output = Result<Table, DatabaseError>> + Send;

    /// Set the order of a database's tables (its tabs). `table_ids` names
    /// every table exactly once; answers the tables in their new order.
    fn reorder_tables(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        table_ids: Vec<TableId>,
    ) -> impl Future<Output = Result<Vec<Table>, DatabaseError>> + Send;

    /// Remove a table with its rows and columns. A database keeps at least
    /// one table.
    fn delete_table(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        table_id: TableId,
    ) -> impl Future<Output = Result<(), DatabaseError>> + Send;

    /// Add a column.
    fn create_column(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        viewer: Viewer,
        cmd: CreateColumn,
    ) -> impl Future<Output = Result<ColumnId, DatabaseError>> + Send;

    /// Rename a column.
    fn rename_column(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        table_id: TableId,
        column_id: ColumnId,
        name: String,
        previous_name: String,
    ) -> impl Future<Output = Result<RenameColumnOutcome, DatabaseError>> + Send;

    /// Settle an untyped column's type from its first value.
    fn infer_column_type(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        viewer: Viewer,
        cmd: InferColumnType,
    ) -> impl Future<Output = Result<InferColumnTypeOutcome, DatabaseError>> + Send;

    /// Convert a column to another type, converting its cells.
    fn change_column_type(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        viewer: Viewer,
        cmd: ChangeColumnType,
    ) -> impl Future<Output = Result<ColumnSchemaOutcome, DatabaseError>> + Send;

    /// Remove a column.
    fn delete_column(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        table_id: TableId,
        column_id: ColumnId,
        base_version: TableVersion,
    ) -> impl Future<Output = Result<ColumnSchemaOutcome, DatabaseError>> + Send;

    /// Reorder a table's columns.
    fn reorder_columns(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        table_id: TableId,
        column_ids: Vec<ColumnId>,
        base_version: TableVersion,
    ) -> impl Future<Output = Result<ColumnSchemaOutcome, DatabaseError>> + Send;

    /// Add options to a select column.
    fn add_column_options(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        viewer: Viewer,
        cmd: AddColumnOptions,
    ) -> impl Future<Output = Result<ColumnDetail, DatabaseError>> + Send;

    /// Run one statement, reads or writes.
    fn exec_sql(
        &self,
        viewer: Viewer,
        req: ExecRequest,
    ) -> impl Future<Output = Result<ExecOutcome, QueryError>> + Send;

    /// Tell the database's other viewers where this viewer is. Best effort:
    /// a relay failure is logged, never surfaced.
    fn share_awareness(
        &self,
        receipt: EntityAccessReceipt<ViewAccessLevel>,
        viewer: Viewer,
        state: Awareness,
    ) -> impl Future<Output = Result<(), DatabaseError>> + Send;

    /// Run one read-only statement.
    fn query_sql(
        &self,
        viewer: Viewer,
        sql: String,
    ) -> impl Future<Output = Result<ExecOutcome, QueryError>> + Send;

    /// Save a read-only query. It must compile as a SELECT against the
    /// viewer's catalog, scoped to `database_id`, which the viewer must be
    /// able to see.
    fn save_query(
        &self,
        viewer: Viewer,
        database_id: Option<DatabaseId>,
        definition: QueryDefinition,
    ) -> impl Future<Output = Result<SavedQuery, QueryError>> + Send;

    /// A saved query's definition, readable by its creator and by anyone who
    /// can view the live database it is scoped to. Anyone else gets
    /// [`QueryError::NotFound`], so query ids cannot be probed.
    fn get_query(
        &self,
        viewer: Viewer,
        id: QueryId,
    ) -> impl Future<Output = Result<SavedQuery, QueryError>> + Send;

    /// Run a saved query as the viewer, under the same read rule as
    /// [`Self::get_query`]; what it returns is permission-filtered.
    fn run_query(
        &self,
        viewer: Viewer,
        id: QueryId,
    ) -> impl Future<Output = Result<ExecOutcome, QueryError>> + Send;
}
