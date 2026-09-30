//! Axum router for the databases endpoints.
//!
//! The surface is deliberately tiny and SQL-first:
//!
//! - `POST /exec` — run SQL (reads and writes) as the caller; the viewer's
//!   catalog is the authorization boundary, enforced in the domain service.
//! - `POST /query` — read-only SQL for live chips and query previews.
//! - `POST /queries`, `GET /queries/{query_id}`, `POST /queries/{query_id}/run`
//!   — saved, immutable queries that document nodes point at.
//! - `GET /` — list the caller's databases; `POST /` — create one.
//! - `GET /{id}` — schema detail (tables, columns, definitions, SQL names).
//! - `POST /{id}/tables`, `POST /{id}/tables/{table_id}/columns`,
//!   `POST /{id}/tables/{table_id}/columns/{column_id}/options` — schema
//!   operations, which stay structured because property definitions carry
//!   configuration DDL cannot express.
//!
//! Handlers are thin: extract identity/receipts, convert DTOs, call the
//! service, map errors. No policy, no persistence.

use std::collections::HashMap;
/// Structured column type, ordering, and placement deletion endpoints.
pub mod column_mutations;
/// Saved, immutable queries that document nodes point at.
pub mod saved_queries;
/// Native database recipient sharing.
pub mod sharing;
#[cfg(test)]
mod test;
/// Atomic table imports.
pub mod transfer;
use crate::domain::sharing::DatabaseSharingService;
use crate::domain::transfer::DatabaseTransferService;
use column_mutations::{
    change_column_type_handler, column_casts_handler, delete_column_handler,
    reorder_columns_handler,
};
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, patch, post, put},
};
use entity_access::domain::models::{EditAccessLevel, ViewAccessLevel};
use entity_access::domain::ports::EntityAccessService;
use entity_access::inbound::axum_extractors::DatabaseAccessLevelExtractor;
use macro_authorization::{
    MacroAuthorizationExtractor, MacroAuthorizationService, MacroAuthorizationState, UserOrInternal,
};
use model_error_response::ErrorResponse;
use models_properties::shared::DataType;
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::models::{
    AddColumnOptions, Awareness, ColumnBinding, ColumnConfig, ColumnDetail, ColumnId, CreateColumn,
    CreateDatabase, CreateTable, Database, DatabaseDetail, DatabaseError, ExecOutcome, ExecRequest,
    InferColumnType, InferColumnTypeOutcome, ListedDatabase, QueryError, RenameColumnOutcome,
    Table, TableVersion, Viewer,
};
use crate::domain::ports::DatabasesService;

/// Largest table import accepted, in bytes.
const MAX_IMPORT_BODY_BYTES: usize = 16 * 1024 * 1024;

/// Router state for databases endpoints.
pub struct DatabasesRouterState<S, Eas, Auth> {
    service: Arc<S>,
    entity_access_service: Arc<Eas>,
    authorization_state: MacroAuthorizationState<Auth>,
}

impl<S, Eas, Auth> Clone for DatabasesRouterState<S, Eas, Auth> {
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            entity_access_service: self.entity_access_service.clone(),
            authorization_state: self.authorization_state.clone(),
        }
    }
}

impl<S, Eas, Auth> DatabasesRouterState<S, Eas, Auth>
where
    S: DatabasesService,
    Eas: EntityAccessService,
{
    /// Create router state from shared service references and authorization state.
    pub fn new(
        service: Arc<S>,
        entity_access_service: Arc<Eas>,
        authorization_state: MacroAuthorizationState<Auth>,
    ) -> Self {
        Self {
            service,
            entity_access_service,
            authorization_state,
        }
    }
}

impl<S, Eas, Auth> FromRef<DatabasesRouterState<S, Eas, Auth>> for Arc<Eas> {
    fn from_ref(state: &DatabasesRouterState<S, Eas, Auth>) -> Self {
        state.entity_access_service.clone()
    }
}

impl<S, Eas, Auth> FromRef<DatabasesRouterState<S, Eas, Auth>> for MacroAuthorizationState<Auth> {
    fn from_ref(state: &DatabasesRouterState<S, Eas, Auth>) -> Self {
        state.authorization_state.clone()
    }
}

/// Build the databases router.
pub fn databases_router<S, Eas, Auth, T>(state: DatabasesRouterState<S, Eas, Auth>) -> Router<T>
where
    S: DatabasesService + DatabaseSharingService + DatabaseTransferService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
    T: Send + Sync + 'static,
{
    Router::new()
        .route("/", get(list_databases_handler::<S, Eas, Auth>))
        .route(
            "/{id}/import",
            post(transfer::import_table_handler::<S, Eas, Auth>)
                .layer(axum::extract::DefaultBodyLimit::max(MAX_IMPORT_BODY_BYTES)),
        )
        .route("/", post(create_database_handler::<S, Eas, Auth>))
        .route("/exec", post(exec_handler::<S, Eas, Auth>))
        .route("/query", post(query_handler::<S, Eas, Auth>))
        // Static segments win over `/{id}`, so these never read as a database.
        .route(
            "/queries",
            post(saved_queries::save_query_handler::<S, Eas, Auth>),
        )
        .route(
            "/queries/{query_id}",
            get(saved_queries::get_query_handler::<S, Eas, Auth>),
        )
        .route(
            "/queries/{query_id}/run",
            post(saved_queries::run_query_handler::<S, Eas, Auth>),
        )
        .route("/{id}", get(get_database_handler::<S, Eas, Auth>))
        .route("/{id}/awareness", put(awareness_handler::<S, Eas, Auth>))
        .route(
            "/{id}/permissions",
            get(sharing::get_permissions_handler::<S, Eas, Auth>)
                .patch(sharing::update_permissions_handler::<S, Eas, Auth>),
        )
        .route("/{id}/tables", post(create_table_handler::<S, Eas, Auth>))
        // Static, so it never reads as a table id.
        .route(
            "/{id}/tables/order",
            put(reorder_tables_handler::<S, Eas, Auth>),
        )
        .route(
            "/{id}/tables/{table_id}",
            patch(rename_table_handler::<S, Eas, Auth>)
                .delete(delete_table_handler::<S, Eas, Auth>),
        )
        .route(
            "/{id}/tables/{table_id}/columns",
            post(create_column_handler::<S, Eas, Auth>),
        )
        .route(
            "/{id}/tables/{table_id}/columns/{column_id}",
            patch(rename_column_handler::<S, Eas, Auth>)
                .delete(delete_column_handler::<S, Eas, Auth>),
        )
        .route(
            "/{id}/tables/{table_id}/columns/{column_id}/type",
            patch(change_column_type_handler::<S, Eas, Auth>),
        )
        .route(
            "/{id}/tables/{table_id}/columns/{column_id}/casts",
            get(column_casts_handler::<S, Eas, Auth>),
        )
        .route(
            "/{id}/tables/{table_id}/columns/order",
            patch(reorder_columns_handler::<S, Eas, Auth>),
        )
        .route(
            "/{id}/tables/{table_id}/columns/{column_id}/infer-type",
            post(infer_column_type_handler::<S, Eas, Auth>),
        )
        .route(
            "/{id}/tables/{table_id}/columns/{column_id}/options",
            post(add_column_options_handler::<S, Eas, Auth>),
        )
        .with_state(state)
}

fn viewer_of<Auth>(user: &MacroAuthorizationExtractor<Auth, UserOrInternal>) -> Viewer {
    Viewer {
        user_id: user.authorization.user.macro_user_id.clone(),
        acting_bot: None,
    }
}

/// Request body for creating a database.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateDatabaseRequest {
    /// Display name.
    pub name: String,
}

/// Request body for creating a table.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateTableRequest {
    /// Display name.
    pub name: String,
}

/// Request body for renaming a table without overwriting a concurrent rename.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RenameTableRequest {
    /// New display name.
    pub name: String,
    /// Name shown when the rename editor opened.
    pub previous_name: String,
}

/// A complete tab order, identified by stable table IDs.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReorderTablesRequest {
    /// Every table of the database, exactly once, in the new left-to-right order.
    pub table_ids: Vec<Uuid>,
}

/// Rename one column placement without changing its property's SQL identifier.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RenameColumnRequest {
    /// New display name.
    pub name: String,
    /// Label shown when the rename editor opened.
    pub previous_name: String,
}

/// How a new column obtains its definition.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ColumnBindingRequest {
    /// Create a fresh definition scoped to the database.
    New {
        /// Column display name.
        name: String,
        /// Value type.
        data_type: DataType,
        /// Whether the column holds multiple values.
        #[serde(default)]
        is_multi_select: bool,
        /// For a select or tag column, the labels SQL will accept. A select
        /// column created without any accepts nothing until options are added.
        #[serde(default)]
        #[schema(nullable = false)]
        options: Option<Vec<String>>,
    },
    /// Bind an existing user/team/system definition.
    Existing {
        /// The definition to bind.
        property_definition_id: Uuid,
    },
}

/// Request body for creating a column.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateColumnRequest {
    /// Infer the first value type of a newly owned text column. `infer_type`
    /// is still accepted from clients that predate the camelCase name.
    #[serde(default, alias = "infer_type")]
    pub infer_type: bool,
    /// Definition source.
    pub binding: ColumnBindingRequest,
    /// Link this column to another table (many-to-many).
    #[schema(nullable = false)]
    pub link_to_table_id: Option<Uuid>,
    /// Database of the linked table (defaults to this database).
    #[schema(nullable = false)]
    pub link_to_database_id: Option<Uuid>,
}

/// Request body for `POST /exec`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecRequestBody {
    /// The statements to run, executed in one transaction.
    pub sql: String,
    /// Optional compare-and-swap: reject writes if a listed table being written
    /// has moved past the given version. Read-only dependencies are not guarded.
    /// Omitted → cell-level last-write-wins.
    #[schema(nullable = false)]
    pub base_versions: Option<HashMap<Uuid, i64>>,
    /// The database the statement is written from. A same-named table of
    /// another database is left out of name resolution when this is set.
    #[serde(default)]
    #[schema(nullable = false)]
    pub scope: Option<Uuid>,
}

/// Request body for read-only SQL queries.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct QueryRequestBody {
    /// SQL to read. Writes are refused by the domain service.
    pub sql: String,
}

/// Path params for the single-database routes.
#[derive(Debug, Deserialize)]
pub struct DatabasePath {
    /// Database id.
    pub id: Uuid,
}

/// Path params for the column route.
#[derive(Debug, Deserialize)]
pub struct ColumnPath {
    /// Database id.
    pub id: Uuid,
    /// Table id.
    pub table_id: Uuid,
}

/// Path params for the column-options route.
#[derive(Debug, Deserialize)]
pub struct ColumnOptionsPath {
    /// Database id.
    pub id: Uuid,
    /// Table id.
    pub table_id: Uuid,
    /// Column id.
    pub column_id: Uuid,
}

/// Request body for adding options to a select column.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AddColumnOptionsRequest {
    /// Display labels to add. Labels the column already has are ignored.
    pub labels: Vec<String>,
}

/// List the caller's databases.
#[utoipa::path(
    get,
    tag = "databases",
    operation_id = "list_databases",
    path = "/databases",
    responses(
        (status = 200, body = Vec<ListedDatabase>),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn list_databases_handler<S, Eas, Auth>(
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
) -> Result<Json<Vec<ListedDatabase>>, DatabaseError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    let databases = state.service.list_databases(viewer_of(&user)).await?;
    Ok(Json(databases))
}

/// Create a database owned by the caller.
#[utoipa::path(
    post,
    tag = "databases",
    operation_id = "create_database",
    path = "/databases",
    request_body = CreateDatabaseRequest,
    responses(
        (status = 201, body = Database),
        (status = 400, body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn create_database_handler<S, Eas, Auth>(
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Json(req): Json<CreateDatabaseRequest>,
) -> Result<(StatusCode, Json<Database>), DatabaseError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    let owner_id = user.authorization.user.macro_user_id.clone();
    let database = state
        .service
        .create_database(CreateDatabase {
            name: req.name,
            owner_id,
            acting_bot: None,
        })
        .await?;
    Ok((StatusCode::CREATED, Json(database)))
}

/// Schema detail of one database.
#[utoipa::path(
    get,
    tag = "databases",
    operation_id = "get_database",
    path = "/databases/{id}",
    params(("id" = Uuid, Path, description = "Database id")),
    responses(
        (status = 200, body = DatabaseDetail),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "No access to the database", body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn get_database_handler<S, Eas, Auth>(
    access: DatabaseAccessLevelExtractor<ViewAccessLevel, Eas, Auth>,
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
) -> Result<Json<DatabaseDetail>, DatabaseError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    let detail = state
        .service
        .get_database(access.entity_access_receipt, viewer_of(&user))
        .await?;
    Ok(Json(detail))
}

/// Tell a database's other viewers where the caller is.
#[utoipa::path(
    put,
    tag = "databases",
    operation_id = "share_database_awareness",
    path = "/databases/{id}/awareness",
    params(("id" = Uuid, Path, description = "Database id")),
    request_body = Awareness,
    responses(
        (status = 204, description = "Relayed"),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "No access to the database", body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn awareness_handler<S, Eas, Auth>(
    access: DatabaseAccessLevelExtractor<ViewAccessLevel, Eas, Auth>,
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Json(awareness): Json<Awareness>,
) -> Result<StatusCode, DatabaseError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    state
        .service
        .share_awareness(access.entity_access_receipt, viewer_of(&user), awareness)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Execute SQL as the caller. The whole read/write surface.
#[utoipa::path(
    post,
    tag = "databases",
    operation_id = "exec_database_sql",
    path = "/databases/exec",
    request_body = ExecRequestBody,
    responses(
        (status = 200, body = ExecOutcome),
        (status = 400, description = "SQL error (message verbatim from the SQL engine) or a refused write", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "Write to a read-only table or column", body = ErrorResponse),
        (status = 409, description = "A written table moved past its base version", body = ErrorResponse),
        (status = 422, description = "Query budget exceeded", body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn exec_handler<S, Eas, Auth>(
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Json(req): Json<ExecRequestBody>,
) -> Result<Json<ExecOutcome>, QueryError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    let outcome = state
        .service
        .exec_sql(
            viewer_of(&user),
            ExecRequest {
                scope: req.scope,
                sql: req.sql,
                base_versions: req.base_versions.map(|versions| {
                    versions
                        .into_iter()
                        .map(|(table, version)| (table, TableVersion(version)))
                        .collect()
                }),
            },
        )
        .await?;
    Ok(Json(outcome))
}

/// Run read-only SQL with the caller's current visibility.
#[utoipa::path(
    post,
    tag = "databases",
    operation_id = "query_database_sql",
    path = "/databases/query",
    request_body = QueryRequestBody,
    responses(
        (status = 200, body = ExecOutcome),
        (status = 400, description = "Invalid SQL", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "Queries cannot write data", body = ErrorResponse),
        (status = 422, description = "Query budget exceeded", body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn query_handler<S, Eas, Auth>(
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Json(req): Json<QueryRequestBody>,
) -> Result<Json<ExecOutcome>, QueryError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    state
        .service
        .query_sql(viewer_of(&user), req.sql)
        .await
        .map(Json)
}

/// Create a table in a database.
#[utoipa::path(
    post,
    tag = "databases",
    operation_id = "create_database_table",
    path = "/databases/{id}/tables",
    params(("id" = Uuid, Path, description = "Database id")),
    request_body = CreateTableRequest,
    responses(
        (status = 201, body = Table),
        (status = 400, body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "No edit access to the database", body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn create_table_handler<S, Eas, Auth>(
    access: DatabaseAccessLevelExtractor<EditAccessLevel, Eas, Auth>,
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    Path(DatabasePath { id }): Path<DatabasePath>,
    Json(req): Json<CreateTableRequest>,
) -> Result<(StatusCode, Json<Table>), DatabaseError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    let table = state
        .service
        .create_table(
            access.entity_access_receipt,
            CreateTable {
                database_id: id,
                name: req.name,
            },
        )
        .await?;
    Ok((StatusCode::CREATED, Json(table)))
}

/// Rename a table in a database.
#[utoipa::path(
    patch,
    tag = "databases",
    operation_id = "rename_database_table",
    path = "/databases/{id}/tables/{table_id}",
    params(("id" = Uuid, Path, description = "Database id"),
           ("table_id" = Uuid, Path, description = "Table id")),
    request_body = RenameTableRequest,
    responses(
        (status = 200, body = Table),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn rename_table_handler<S, Eas, Auth>(
    access: DatabaseAccessLevelExtractor<EditAccessLevel, Eas, Auth>,
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    Path(ColumnPath { table_id, .. }): Path<ColumnPath>,
    Json(req): Json<RenameTableRequest>,
) -> Result<Json<Table>, DatabaseError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    state
        .service
        .rename_table(
            access.entity_access_receipt,
            table_id,
            req.name,
            req.previous_name,
        )
        .await
        .map(Json)
}

/// Set the order of a database's tables (its tabs).
#[utoipa::path(
    put,
    tag = "databases",
    operation_id = "reorder_database_tables",
    path = "/databases/{id}/tables/order",
    params(("id" = Uuid, Path, description = "Database id")),
    request_body = ReorderTablesRequest,
    responses(
        (status = 200, description = "The tables in their new order", body = Vec<Table>),
        (status = 400, description = "The order does not name every table exactly once", body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, description = "The database's tables changed while the order was written", body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn reorder_tables_handler<S, Eas, Auth>(
    access: DatabaseAccessLevelExtractor<EditAccessLevel, Eas, Auth>,
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    Json(req): Json<ReorderTablesRequest>,
) -> Result<Json<Vec<Table>>, DatabaseError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    state
        .service
        .reorder_tables(access.entity_access_receipt, req.table_ids)
        .await
        .map(Json)
}

/// Delete a table with its rows and columns. A database keeps at least one.
#[utoipa::path(
    delete,
    tag = "databases",
    operation_id = "delete_database_table",
    path = "/databases/{id}/tables/{table_id}",
    params(("id" = Uuid, Path, description = "Database id"),
           ("table_id" = Uuid, Path, description = "Table id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 400, description = "The last table, or a relation still points at it", body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn delete_table_handler<S, Eas, Auth>(
    access: DatabaseAccessLevelExtractor<EditAccessLevel, Eas, Auth>,
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    Path(ColumnPath { table_id, .. }): Path<ColumnPath>,
) -> Result<StatusCode, DatabaseError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    state
        .service
        .delete_table(access.entity_access_receipt, table_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Response for a created column.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateColumnResponse {
    /// Identifier of the new column placement.
    #[schema(value_type = String, format = Uuid)]
    pub column_id: ColumnId,
}

/// Add a column to a table.
#[utoipa::path(
    post,
    tag = "databases",
    operation_id = "create_database_column",
    path = "/databases/{id}/tables/{table_id}/columns",
    params(
        ("id" = Uuid, Path, description = "Database id"),
        ("table_id" = Uuid, Path, description = "Table id"),
    ),
    request_body = CreateColumnRequest,
    responses(
        (status = 201, body = CreateColumnResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "No edit access to the database", body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn create_column_handler<S, Eas, Auth>(
    access: DatabaseAccessLevelExtractor<EditAccessLevel, Eas, Auth>,
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(ColumnPath { id, table_id }): Path<ColumnPath>,
    Json(req): Json<CreateColumnRequest>,
) -> Result<(StatusCode, Json<CreateColumnResponse>), DatabaseError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    let binding = match req.binding {
        ColumnBindingRequest::New {
            name,
            data_type,
            is_multi_select,
            options,
        } => ColumnBinding::NewDefinition {
            name,
            data_type,
            is_multi_select,
            options: options.unwrap_or_default(),
        },
        ColumnBindingRequest::Existing {
            property_definition_id,
        } => ColumnBinding::ExistingDefinition(property_definition_id),
    };
    let config = req.link_to_table_id.map(|target| ColumnConfig::Link {
        database_id: req.link_to_database_id.unwrap_or(id),
        table_id: target,
    });
    let column_id = state
        .service
        .create_column(
            access.entity_access_receipt,
            viewer_of(&user),
            CreateColumn {
                infer_type: req.infer_type,
                table_id,
                binding,
                config,
            },
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(CreateColumnResponse { column_id }),
    ))
}

/// Rename a column's label in this table.
#[utoipa::path(
    patch,
    tag = "databases",
    operation_id = "rename_database_column",
    path = "/databases/{id}/tables/{table_id}/columns/{column_id}",
    params(("id" = Uuid, Path, description = "Database id"),
           ("table_id" = Uuid, Path, description = "Table id"),
           ("column_id" = Uuid, Path, description = "Column id")),
    request_body = RenameColumnRequest,
    responses(
        (status = 200, body = RenameColumnOutcome),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn rename_column_handler<S, Eas, Auth>(
    access: DatabaseAccessLevelExtractor<EditAccessLevel, Eas, Auth>,
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    Path(ColumnOptionsPath {
        table_id,
        column_id,
        ..
    }): Path<ColumnOptionsPath>,
    Json(req): Json<RenameColumnRequest>,
) -> Result<Json<RenameColumnOutcome>, DatabaseError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    state
        .service
        .rename_column(
            access.entity_access_receipt,
            table_id,
            column_id,
            req.name,
            req.previous_name,
        )
        .await
        .map(Json)
}

/// Request to settle an empty column's first-value type.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct InferColumnTypeRequest {
    /// First-value type: STRING, NUMBER, or ENTITY.
    pub data_type: DataType,
    /// Required entity category for ENTITY.
    pub specific_entity_type: Option<models_properties::EntityType>,
    /// Table version used when interpreting the first value.
    pub base_version: TableVersion,
}

/// Settle a new empty text column's type.
#[utoipa::path(
    post,
    tag = "databases",
    operation_id = "infer_database_column_type",
    path = "/databases/{id}/tables/{table_id}/columns/{column_id}/infer-type",
    params(("id" = Uuid, Path, description = "Database id"),
           ("table_id" = Uuid, Path, description = "Table id"),
           ("column_id" = Uuid, Path, description = "Column id")),
    request_body = InferColumnTypeRequest,
    responses((status = 200, body = InferColumnTypeOutcome),
              (status = 400, body = ErrorResponse), (status = 401, body = ErrorResponse),
              (status = 403, body = ErrorResponse), (status = 404, body = ErrorResponse),
              (status = 409, body = ErrorResponse),
              (status = 500, body = ErrorResponse))
)]
#[tracing::instrument(err, skip_all)]
pub async fn infer_column_type_handler<S, Eas, Auth>(
    access: DatabaseAccessLevelExtractor<EditAccessLevel, Eas, Auth>,
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(ColumnOptionsPath {
        table_id,
        column_id,
        ..
    }): Path<ColumnOptionsPath>,
    Json(req): Json<InferColumnTypeRequest>,
) -> Result<Json<InferColumnTypeOutcome>, DatabaseError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    state
        .service
        .infer_column_type(
            access.entity_access_receipt,
            viewer_of(&user),
            InferColumnType {
                table_id,
                column_id,
                data_type: req.data_type,
                specific_entity_type: req.specific_entity_type,
                base_version: req.base_version,
            },
        )
        .await
        .map(Json)
}

/// Add options to a select column.
#[utoipa::path(
    post,
    tag = "databases",
    operation_id = "add_database_column_options",
    path = "/databases/{id}/tables/{table_id}/columns/{column_id}/options",
    params(
        ("id" = Uuid, Path, description = "Database id"),
        ("table_id" = Uuid, Path, description = "Table id"),
        ("column_id" = Uuid, Path, description = "Column id"),
    ),
    request_body = AddColumnOptionsRequest,
    responses(
        (status = 200, body = ColumnDetail),
        (status = 400, description = "Not a select column, or an invalid label", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials", body = ErrorResponse),
        (status = 403, description = "No edit access to the database", body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    )
)]
#[tracing::instrument(err, skip_all)]
pub async fn add_column_options_handler<S, Eas, Auth>(
    access: DatabaseAccessLevelExtractor<EditAccessLevel, Eas, Auth>,
    State(state): State<DatabasesRouterState<S, Eas, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(ColumnOptionsPath {
        id: _,
        table_id,
        column_id,
    }): Path<ColumnOptionsPath>,
    Json(req): Json<AddColumnOptionsRequest>,
) -> Result<Json<ColumnDetail>, DatabaseError>
where
    S: DatabasesService,
    Eas: EntityAccessService,
    Auth: MacroAuthorizationService,
{
    let column = state
        .service
        .add_column_options(
            access.entity_access_receipt,
            viewer_of(&user),
            AddColumnOptions {
                table_id,
                column_id,
                labels: req.labels,
            },
        )
        .await?;
    Ok(Json(column))
}

impl IntoResponse for DatabaseError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            DatabaseError::NotFound => StatusCode::NOT_FOUND,
            DatabaseError::Unauthorized => StatusCode::FORBIDDEN,
            DatabaseError::VersionConflict => StatusCode::CONFLICT,
            DatabaseError::InvalidSchemaOperation(_) | DatabaseError::InvalidSharing(_) => {
                StatusCode::BAD_REQUEST
            }
            DatabaseError::Repo(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let message = match &self {
            DatabaseError::Repo(_) => {
                tracing::error!(error = ?self, "databases internal server error");
                "internal server error".to_string()
            }
            other => other.to_string(),
        };
        (
            status,
            Json(ErrorResponse {
                message: message.into(),
            }),
        )
            .into_response()
    }
}

impl IntoResponse for QueryError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            // The engine's message is the product's "broken query" state —
            // pass it through verbatim for chips to render.
            QueryError::Sql(_) => StatusCode::BAD_REQUEST,
            QueryError::ReadOnly(_) => StatusCode::FORBIDDEN,
            QueryError::VersionConflict { .. } => StatusCode::CONFLICT,
            QueryError::BudgetExceeded => StatusCode::UNPROCESSABLE_ENTITY,
            QueryError::NotFound => StatusCode::NOT_FOUND,
            QueryError::Infrastructure(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let message = match &self {
            QueryError::Infrastructure(_) => {
                tracing::error!(error = ?self, "databases query infrastructure error");
                "internal server error".to_string()
            }
            other => other.to_string(),
        };
        (
            status,
            Json(ErrorResponse {
                message: message.into(),
            }),
        )
            .into_response()
    }
}
