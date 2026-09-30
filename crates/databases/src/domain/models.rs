//! Domain models: entities, commands, the query/exec pipeline vocabulary,
//! and domain errors.

use std::collections::HashMap;

use bot_id::BotId;
use chrono::{DateTime, Utc};
use macro_user_id::user_id::MacroUserIdStr;
use model_entity::EntityType;
use models_properties::service::property_value::PropertyValue;
use models_properties::shared::DataType;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ===== Identifiers =====

/// Identifier of a database (the shareable entity users see).
pub type DatabaseId = Uuid;
/// Identifier of one table (tab) within a database.
pub type TableId = Uuid;
/// Identifier of a column placement within a table.
pub type ColumnId = Uuid;
/// Identifier of a row.
pub type RowId = Uuid;
/// Identifier of a `models_properties` property definition bound as a column.
pub type PropertyDefinitionId = Uuid;

/// Monotonic per-table version, bumped on every row/column/link mutation.
///
/// The cache key for query materializations and the invalidation signal for
/// live query chips.
#[derive(
    utoipa::ToSchema, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct TableVersion(pub i64);

// ===== Entities =====

/// A database: a named collection of tables, owned and shared as one entity.
#[derive(utoipa::ToSchema, Debug, Clone, Serialize, Deserialize)]
pub struct Database {
    /// Identifier.
    #[schema(value_type = Uuid)]
    pub id: DatabaseId,
    /// Display name.
    pub name: String,
    /// Owning user.
    pub owner_id: String,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Set when trashed.
    pub trashed_at: Option<DateTime<Utc>>,
}

/// One table (tab) of a database.
#[derive(utoipa::ToSchema, Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    /// Identifier.
    #[schema(value_type = Uuid)]
    pub id: TableId,
    /// Owning database.
    #[schema(value_type = Uuid)]
    pub database_id: DatabaseId,
    /// Display name; also the basis of the table's SQL name.
    pub name: String,
    /// Fractional index for tab ordering.
    pub position: String,
    /// Current version.
    pub version: TableVersion,
}

/// A column: the placement of a property definition on a table.
///
/// The definition carries name, [`DataType`], multi-select flag, and options;
/// this carries only where it appears and column-kind configuration.
#[derive(utoipa::ToSchema, Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    /// Identifier of the placement.
    #[schema(value_type = Uuid)]
    pub id: ColumnId,
    /// Table the column appears on.
    #[schema(value_type = Uuid)]
    pub table_id: TableId,
    /// The bound property definition.
    #[schema(value_type = Uuid)]
    pub property_definition_id: PropertyDefinitionId,
    /// Fractional index for column ordering.
    pub position: String,
    /// Column-kind specific configuration.
    pub config: Option<ColumnConfig>,
    /// Optional label for this placement. The property's name still defines
    /// its SQL identifier, so renaming a column does not break saved queries.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Whether the first nonempty value may settle this new text column's type.
    #[serde(default)]
    pub infer_type: bool,
}

/// A renamed placement and its table's version after the atomic update.
#[derive(utoipa::ToSchema, Debug, Clone, Serialize, Deserialize)]
pub struct RenameColumnOutcome {
    /// The placement with its new display label; IDs and binding are preserved.
    pub column: Column,
    /// Monotonic table version used to reconcile concurrent client refreshes.
    pub table_version: TableVersion,
}

/// Explicit type selection for one column placement, guarded by its table version.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeColumnType {
    /// Owning table.
    pub table_id: TableId,
    /// Placement to change; its identity and label remain stable.
    pub column_id: ColumnId,
    /// Requested property type.
    pub data_type: DataType,
    /// Whether select/entity/link values may contain multiple items.
    pub is_multi_select: bool,
    /// Entity category for a Macro entity reference.
    pub specific_entity_type: Option<models_properties::EntityType>,
    /// Optional database-row relationship target.
    pub relation: Option<(DatabaseId, TableId)>,
    /// Snapshot against which values are converted.
    pub base_version: TableVersion,
}

/// Fully validated replacement values for an atomic column rebind.
#[derive(Debug)]
pub struct ColumnReplacement {
    /// Existing placement and binding, used as a compare-and-swap guard.
    pub column: Column,
    /// Fresh definition owned by this database.
    pub definition_id: PropertyDefinitionId,
    /// Requested relationship configuration, if any.
    pub config: Option<ColumnConfig>,
    /// Converted values for every nonempty source cell, keyed by row identity.
    pub values: Vec<(RowId, PropertyValue)>,
}

/// Table versions changed by a placement deletion or reorder.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ColumnSchemaOutcome {
    /// Includes both endpoint tables when deleting relationship edges.
    #[schema(value_type = HashMap<String, TableVersion>)]
    pub table_versions: HashMap<TableId, TableVersion>,
}

/// Column-kind specific configuration stored on the placement.
#[derive(utoipa::ToSchema, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ColumnConfig {
    /// A link column targeting another table; edges live in the junction.
    Link {
        /// Target database.
        #[schema(value_type = Uuid)]
        database_id: DatabaseId,
        /// Target table.
        #[schema(value_type = Uuid)]
        table_id: TableId,
    },
    /// A derived lookup through a link or entity column on the same table.
    Lookup {
        /// The link/entity column the lookup reads through.
        #[schema(value_type = Uuid)]
        via_column_id: ColumnId,
        /// Target field on the other side (a definition id or magic column name).
        target: String,
    },
}

/// A row's identity and place in its table. Cells are not here: they are
/// entity properties of the `DATABASE_ROW` entity the id names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowRef {
    /// Identifier.
    pub id: RowId,
    /// Fractional index for manual ordering.
    pub position: String,
}

// ===== Schema operations (the structured, non-SQL part of the API) =====

/// Command to create a database (with one starter table).
#[derive(Debug, Clone)]
pub struct CreateDatabase {
    /// Display name.
    pub name: String,
    /// Owner.
    pub owner_id: MacroUserIdStr<'static>,
    /// The agent creating it for the owner; `None` when the owner acts.
    pub acting_bot: Option<BotId>,
}

/// Command to create a table in a database.
#[derive(Debug, Clone)]
pub struct CreateTable {
    /// Owning database.
    pub database_id: DatabaseId,
    /// Display name.
    pub name: String,
}

/// Result of a table mutation checked atomically against its parent database.
#[derive(Debug, Clone)]
pub enum TableMutationOutcome {
    /// The table was created or renamed and the transaction committed.
    Applied(Table),
    /// The database was missing or trashed at the write boundary.
    NotFound,
    /// The name was taken or the table's previous name no longer matched.
    Conflict,
}

/// How a new column obtains its property definition.
#[derive(Debug, Clone)]
pub enum ColumnBinding {
    /// Create a fresh definition scoped to the database.
    NewDefinition {
        /// Column display name.
        name: String,
        /// Value type.
        data_type: DataType,
        /// Whether the column holds multiple values.
        is_multi_select: bool,
        /// Display labels of the select options the column accepts, for the
        /// data types that take options ([`DataType::SelectString`],
        /// [`DataType::SelectNumber`], [`DataType::Tag`]). Options are
        /// explicit schema: the compiled SQLite column carries a `CHECK`
        /// listing exactly these labels, so a column created without any
        /// accepts no value at all. Empty for every other data type.
        options: Vec<String>,
    },
    /// Bind an existing user/team/system definition.
    ExistingDefinition(PropertyDefinitionId),
}

/// Command to add a column to a table.
#[derive(Debug, Clone)]
pub struct CreateColumn {
    /// Allow first-value inference for a newly owned plain text column.
    pub infer_type: bool,
    /// Table receiving the column.
    pub table_id: TableId,
    /// Definition source.
    pub binding: ColumnBinding,
    /// Column-kind configuration (links, lookups).
    pub config: Option<ColumnConfig>,
}

/// Settle a new empty column's type using its first value.
#[derive(Debug, Clone)]
pub struct InferColumnType {
    /// Table containing the placement.
    pub table_id: TableId,
    /// Placement to settle.
    pub column_id: ColumnId,
    /// Requested first-value type: string, number, or entity.
    pub data_type: DataType,
    /// Required when the inferred type is an entity reference.
    pub specific_entity_type: Option<models_properties::EntityType>,
    /// Version of the schema used to interpret the first value.
    pub base_version: TableVersion,
}

/// Settled schema and the version against which its first value can be written.
#[derive(utoipa::ToSchema, Debug, Clone, Serialize)]
pub struct InferColumnTypeOutcome {
    /// Updated placement, property definition, and stable SQL identifier.
    pub column: ColumnDetail,
    /// Version after settling the column.
    pub table_version: TableVersion,
}

/// Command to extend a select column's set of allowed options.
///
/// Add-only: an option is never renamed or removed here, because both would
/// change what existing cells mean.
#[derive(Debug, Clone)]
pub struct AddColumnOptions {
    /// Table the column belongs to.
    pub table_id: TableId,
    /// The column to extend.
    pub column_id: ColumnId,
    /// Display labels to add. Labels already on the column are ignored rather
    /// than rejected, so re-sending a list is safe.
    pub labels: Vec<String>,
}

// ===== The query/exec pipeline =====

/// The acting viewer: every catalog build, materialization, and write is
/// scoped to this identity. The catalog IS the authorization for SQL.
#[derive(Debug, Clone)]
pub struct Viewer {
    /// The user running the statement.
    pub user_id: MacroUserIdStr<'static>,
    /// The agent running it for that user; `None` when the user acts. Only
    /// attribution reads it: the catalog stays scoped to `user_id`.
    pub acting_bot: Option<BotId>,
}

/// A request to execute SQL (any mix of reads and writes).
#[derive(Debug, Clone, Deserialize)]
pub struct ExecRequest {
    /// The statements to run, executed in one transaction.
    pub sql: String,
    /// The database the statement is written from, when a client knows it
    /// (the grid always does). Names are resolved against the whole catalog,
    /// but a table of another database whose qualified name collides with
    /// one of this database's is left out, so two databases both called
    /// "Untitled database" with a "Table 1" each stay addressable.
    pub scope: Option<DatabaseId>,
    /// Compare-and-set, **opt in per table**. A written table named here is
    /// refused (nothing commits) unless it is still at the given version;
    /// entries for tables the statement does not write are ignored.
    ///
    /// A written table that is *not* named — including the case where the
    /// whole field is omitted — is committed blind: cell-level
    /// last-write-wins, with no check that the table moved underneath the
    /// caller. Omission is therefore a deliberate choice, correct for a
    /// human typing ad-hoc SQL into the console or for an agent tool, and
    /// wrong for a client re-sending a statement it built from data it
    /// already read. Such a client should send back the
    /// [`ExecOutcome::read_versions`] of the previous run to guard tables
    /// the follow-up statement writes. Read-only dependencies are not guarded.
    pub base_versions: Option<HashMap<TableId, TableVersion>>,
}

/// A value in the SQLite materialization, kept engine-agnostic so the domain
/// never depends on rusqlite types. Serializes as a plain JSON scalar.
#[derive(utoipa::ToSchema, Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum SqlValue {
    /// SQL NULL.
    Null,
    /// Integer (also booleans as 0/1).
    Integer(i64),
    /// Float.
    Real(f64),
    /// Text (also ids, dates as ISO-8601, resolved option display values).
    Text(String),
}

/// A SELECT's result set with provenance for hydration and write-through.
#[derive(utoipa::ToSchema, Debug, Clone, Serialize)]
pub struct QueryResult {
    /// Result columns.
    pub columns: Vec<ResultColumn>,
    /// Row values as JSON scalars.
    pub rows: Vec<Vec<SqlValue>>,
}

/// One result column with its origin.
#[derive(utoipa::ToSchema, Debug, Clone, Serialize)]
pub struct ResultColumn {
    /// Column name or alias.
    pub name: String,
    /// Entity type of id values, when known — drives chip rendering.
    #[schema(inline)]
    pub entity_type: Option<EntityType>,
    /// Origin `(table, column)` when the column traces to a single base
    /// column — the precondition for write-through.
    pub origin: Option<(String, String)>,
}

/// Outcome of an [`ExecRequest`].
#[derive(utoipa::ToSchema, Debug, Clone, Serialize)]
pub struct ExecOutcome {
    /// Result sets of the SELECT statements, in order.
    pub results: Vec<QueryResult>,
    /// How many row changes were applied to Postgres.
    pub changes_applied: usize,
    /// Server-minted ids for rows the statement inserted.
    #[schema(value_type = Vec<Uuid>)]
    pub inserted_row_ids: Vec<RowId>,
    /// New versions of every written table, for client-side liveness.
    #[schema(value_type = HashMap<String, TableVersion>)]
    pub new_versions: HashMap<TableId, TableVersion>,
    /// Dependency set of the statement, for liveness subscription.
    #[schema(value_type = Vec<Uuid>)]
    pub read_tables: Vec<TableId>,
    /// Databases containing the read dependencies, for live subscriptions.
    #[schema(value_type = Vec<Uuid>)]
    pub read_database_ids: Vec<DatabaseId>,
    /// The version every user table in [`ExecOutcome::read_tables`] was at
    /// when this statement materialized it. Send these back as
    /// [`ExecRequest::base_versions`] to guard tables the follow-up statement
    /// writes. Versions for tables it only reads are ignored.
    #[schema(value_type = HashMap<String, TableVersion>)]
    pub read_versions: HashMap<TableId, TableVersion>,
    /// Tables whose read hit the engine's row cap; aggregates over them are
    /// incomplete.
    pub truncated_tables: Vec<String>,
}

// ===== Saved queries =====

/// Identifier of a saved query.
pub type QueryId = Uuid;

/// What a saved query asks. Serialized as `{"version": 1, "query": "<sql>"}`:
/// the integer version tags the shape, so a later version can change the
/// fields without breaking the stored rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryDefinition {
    /// A read-only SQL statement.
    V1 {
        /// The SELECT, in the databases dialect.
        query: String,
    },
}

impl QueryDefinition {
    /// The SQL the definition runs.
    pub fn sql(&self) -> &str {
        match self {
            QueryDefinition::V1 { query } => query,
        }
    }
}

impl Serialize for QueryDefinition {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        match self {
            QueryDefinition::V1 { query } => {
                let mut state = serializer.serialize_struct("QueryDefinition", 2)?;
                state.serialize_field("version", &1u8)?;
                state.serialize_field("query", query)?;
                state.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for QueryDefinition {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Versioned {
            version: u8,
            query: String,
        }
        let versioned = Versioned::deserialize(deserializer)?;
        match versioned.version {
            1 => Ok(QueryDefinition::V1 {
                query: versioned.query,
            }),
            other => Err(serde::de::Error::custom(format!(
                "unsupported query definition version {other}"
            ))),
        }
    }
}

impl utoipa::ToSchema for QueryDefinition {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("QueryDefinition")
    }
}

impl utoipa::PartialSchema for QueryDefinition {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::{ObjectBuilder, Type};
        ObjectBuilder::new()
            .description(Some("A versioned query definition."))
            .property(
                "version",
                ObjectBuilder::new()
                    .schema_type(Type::Integer)
                    .enum_values(Some([1])),
            )
            .required("version")
            .property(
                "query",
                ObjectBuilder::new()
                    .schema_type(Type::String)
                    .description(Some("A read-only SELECT in the databases dialect.")),
            )
            .required("query")
            .into()
    }
}

/// A stored, immutable query. Editing a question saves a new one.
#[derive(utoipa::ToSchema, Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedQuery {
    /// Identifier.
    #[schema(value_type = Uuid)]
    pub id: QueryId,
    /// What it asks.
    pub definition: QueryDefinition,
    /// The database whose tables win name resolution; `null` once that
    /// database is deleted, or when none was given.
    #[schema(value_type = Option<Uuid>)]
    pub database_id: Option<DatabaseId>,
    /// Who saved it.
    pub created_by: String,
    /// When it was saved.
    pub created_at: DateTime<Utc>,
}

/// Result of removing a table, checked atomically against its database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableDeletion {
    /// The table and its row identities are gone; these rows' cells are
    /// the caller's to clear.
    Deleted {
        /// Every row the table held.
        row_ids: Vec<RowId>,
    },
    /// The table, or its live database, was not there.
    NotFound,
    /// It is the database's only table.
    LastTable,
}

// ===== Access & rendering models =====

/// The access a viewer holds on a database, from its `entity_access` rows.
#[derive(
    utoipa::ToSchema, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum AccessGrant {
    /// Read rows and run read-only SQL.
    View,
    /// View plus comments (no additional database rights).
    Comment,
    /// Write rows/links and change the schema.
    Edit,
    /// Everything, including sharing and deletion.
    Owner,
}

impl AccessGrant {
    /// Whether SQL may write to the database's tables.
    pub fn can_write(self) -> bool {
        matches!(self, AccessGrant::Edit | AccessGrant::Owner)
    }

    /// Parse the `AccessLevel` enum text stored in `entity_access`.
    pub fn parse(level: &str) -> Option<Self> {
        match level.to_ascii_lowercase().as_str() {
            "view" => Some(AccessGrant::View),
            "comment" => Some(AccessGrant::Comment),
            "edit" => Some(AccessGrant::Edit),
            "owner" => Some(AccessGrant::Owner),
            _ => None,
        }
    }
}

/// A database as listed for a viewer.
#[derive(utoipa::ToSchema, Debug, Clone, Serialize)]
pub struct ListedDatabase {
    /// The database.
    pub database: Database,
    /// The viewer's access.
    pub grant: AccessGrant,
    /// Tables in tab order, so discovery can find a table independently of
    /// the containing database's display name.
    pub tables: Vec<Table>,
}

/// Everything a client needs to render and edit one database: tables,
/// column placements with their definitions, and the SQL names the query
/// surface exposes them under.
#[derive(utoipa::ToSchema, Debug, Clone, Serialize)]
pub struct DatabaseDetail {
    /// The database.
    pub database: Database,
    /// The viewer's access.
    pub grant: AccessGrant,
    /// Tables in tab order.
    pub tables: Vec<TableDetail>,
}

/// One table with its columns and SQL name.
#[derive(utoipa::ToSchema, Debug, Clone, Serialize)]
pub struct TableDetail {
    /// The table.
    pub table: Table,
    /// The name SQL refers to the table by: its display name, quoted when it
    /// needs it (`FROM "Table 1"`), optionally qualified by the database's.
    pub sql_name: String,
    /// The same name; kept for clients that still distinguish reads.
    pub read_sql_name: String,
    /// Columns in display order.
    pub columns: Vec<ColumnDetail>,
}

/// One column placement with the definition behind it.
#[derive(utoipa::ToSchema, Debug, Clone, Serialize)]
pub struct ColumnDetail {
    /// The placement.
    pub column: Column,
    /// The name SQL refers to the column by: its display name, quoted when it
    /// needs it.
    pub sql_name: String,
    /// The bound definition (name, type, options).
    pub definition:
        models_properties::service::property_definition_with_options::PropertyDefinitionWithOptions,
    /// Whether SQL may write this column.
    pub writable: bool,
}

// ===== Awareness =====

/// Where one viewer is inside a database right now: ephemeral, relayed to
/// the other viewers and never stored. A missing row or column means the
/// viewer is on the table but on no cell.
#[derive(utoipa::ToSchema, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Awareness {
    /// The table the viewer is looking at.
    #[schema(value_type = Uuid)]
    pub table_id: TableId,
    /// The row of the focused cell, if any.
    #[schema(value_type = Option<Uuid>, nullable = false)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_id: Option<RowId>,
    /// The column placement of the focused cell, if any.
    #[schema(value_type = Option<Uuid>, nullable = false)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_id: Option<ColumnId>,
    /// Whether the cell is open for editing.
    #[serde(default)]
    pub editing: bool,
    /// Whether the viewer left the database; other viewers drop their state.
    #[serde(default)]
    pub left: bool,
}

// ===== Errors =====

/// Errors for schema and persistence operations.
#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    /// The database, table, column, or row does not exist (or is invisible
    /// to the viewer, which is deliberately indistinguishable).
    #[error("not found")]
    NotFound,
    /// The caller lacks the permission the operation requires.
    #[error("unauthorized")]
    Unauthorized,
    /// A schema operation was invalid (duplicate placement, bad binding, …).
    #[error("invalid schema operation: {0}")]
    InvalidSchemaOperation(String),
    /// The schema changed after the client read its version.
    #[error("The table changed. Refresh before entering this value.")]
    VersionConflict,
    /// Persistence failure.
    #[error("repository error: {0:?}")]
    Repo(rootcause::Report),
}

/// Errors for SQL analysis, execution, and write-back.
#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    /// The statement did not compile or a write was refused (syntax, an
    /// unknown table or column, a value of the wrong type). Surfaced verbatim
    /// — these errors are the product's "broken query" state.
    #[error("sql error: {0}")]
    Sql(String),
    /// The statement writes to a read-only table (View-only grants) or was
    /// sent through the read-only entry point.
    #[error("read-only: {0}")]
    ReadOnly(String),
    /// `base_versions` was set and a written table has moved.
    #[error("version conflict on table {table_id}")]
    VersionConflict {
        /// The table that changed underneath the caller.
        table_id: TableId,
    },
    /// The statement exceeded the execution budget (time or row caps).
    #[error("query budget exceeded")]
    BudgetExceeded,
    /// The saved query, or the database it is scoped to, does not exist or
    /// is invisible to the viewer.
    #[error("not found")]
    NotFound,
    /// Materialization or apply-side persistence failure.
    #[error("query infrastructure error: {0:?}")]
    Infrastructure(rootcause::Report),
}
