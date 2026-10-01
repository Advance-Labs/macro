//! The router's collaborators, just enough to drive a request: a `valid`
//! bearer token, one fixed grant, and a service counting applied batches.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::routing::post;

use entity_access::domain::models::{
    AccessError, AccessLevel, BotAccessScope, BotId, CallChannelInfo, EditAccessLevel,
    EntityAccessReceipt, EntityPermission, EntityType, OwnerAccessLevel, RequiredPermission,
    TeamRole, UserTeamInfo, ViewAccessLevel,
};
use entity_access::domain::ports::EntityAccessService;
use macro_authorization::{
    InternalAuthConfig, JwtValidator, MacroAuthorizationError, MacroAuthorizationServiceImpl,
    MacroAuthorizationState, NoBotAuthorizer, NoUserApiKeyAuthorizer, ValidatedIdentity,
};
use macro_user_id::lowercased::Lowercase;
use macro_user_id::user_id::{MacroUserId, MacroUserIdStr};
use models_databases::{DatabaseOp, OpResult};
use rootcause::Report;
use uuid::Uuid;

use crate::domain::models::{
    AddColumnOptions, Awareness, ChangeColumnType, ColumnCast, ColumnDetail, ColumnId,
    ColumnSchemaOutcome, ColumnTypeChangeOutcome, CreateColumn, CreateDatabase, CreateTable,
    Database, DatabaseDetail, DatabaseError, DatabaseId, InferColumnType, InferColumnTypeOutcome,
    ListedDatabase, QueryDefinition, QueryId, RenameColumnOutcome, SavedQuery, SavedQueryError,
    Table, TableId, TableVersion, Viewer,
};
use crate::domain::ports::DatabasesService;
use crate::inbound::axum_router::DatabasesRouterState;
use crate::inbound::axum_router::ops::apply_ops_handler;

const USER: &str = "macro|ops-router@macro.com";

/// Accepts the bearer token `valid` as [`USER`].
#[derive(Clone, Copy)]
struct ValidToken;

impl JwtValidator for ValidToken {
    fn validate(&self, jwt: &str) -> Result<ValidatedIdentity, Report<MacroAuthorizationError>> {
        match jwt {
            "valid" => Ok(ValidatedIdentity {
                user_id: USER.to_string(),
                fusion_user_id: "fusion-ops-router".to_string(),
                organization_id: None,
                permissions: None,
            }),
            _ => Err(Report::new(MacroAuthorizationError::InvalidCredentials)),
        }
    }
}

type Authorization = MacroAuthorizationServiceImpl<ValidToken>;

fn authorization_state() -> MacroAuthorizationState<Authorization> {
    MacroAuthorizationState::new(Arc::new(MacroAuthorizationServiceImpl::new(
        ValidToken,
        InternalAuthConfig {
            api_key: "ops-router-internal-key".to_string(),
            default_user_id: None,
        },
        NoBotAuthorizer,
        NoUserApiKeyAuthorizer,
    )))
}

/// The ops route alone, for a caller holding `level` on every database, and
/// the service behind it.
pub(super) fn ops_router(level: AccessLevel) -> (Router, Arc<RecordingService>) {
    let service = Arc::new(RecordingService::default());
    let state = DatabasesRouterState::new(
        service.clone(),
        Arc::new(GrantingAccess(level)),
        authorization_state(),
    );
    let router = Router::new()
        .route(
            "/{id}/ops",
            post(apply_ops_handler::<RecordingService, GrantingAccess, Authorization>),
        )
        .with_state(state);
    (router, service)
}

/// Grants every caller the one level it holds.
#[derive(Clone)]
struct GrantingAccess(AccessLevel);

impl EntityAccessService for GrantingAccess {
    async fn generate_entity_access_receipt<T: RequiredPermission>(
        &self,
        _user_id: &MacroUserId<Lowercase<'_>>,
        _user_org_id: Option<i64>,
        _entity_id: &str,
        _entity_type: EntityType,
    ) -> Result<EntityAccessReceipt<T>, AccessError> {
        unimplemented!("the database extractor reads the permission instead")
    }

    async fn generate_bot_entity_access_receipt<T: RequiredPermission>(
        &self,
        _bot_id: BotId,
        _scope: BotAccessScope,
        _entity_id: &str,
        _entity_type: EntityType,
    ) -> Result<EntityAccessReceipt<T>, AccessError> {
        unimplemented!("no bot calls the ops route here")
    }

    async fn get_access_level(
        &self,
        _user_id: Option<&MacroUserId<Lowercase<'_>>>,
        _entity_id: &str,
        _entity_type: EntityType,
    ) -> Result<Option<AccessLevel>, AccessError> {
        Ok(Some(self.0))
    }

    async fn check_access(
        &self,
        _user_id: Option<&MacroUserId<Lowercase<'_>>>,
        _entity_id: &str,
        _entity_type: EntityType,
        _required_level: AccessLevel,
    ) -> Result<AccessLevel, AccessError> {
        Ok(self.0)
    }

    async fn check_public_access(
        &self,
        _entity_id: &str,
        _entity_type: EntityType,
        _required_level: AccessLevel,
    ) -> Result<AccessLevel, AccessError> {
        Err(AccessError::Unauthorized)
    }

    async fn get_entity_permission(
        &self,
        _user_id: Option<&MacroUserId<Lowercase<'_>>>,
        _entity_id: &str,
        _entity_type: EntityType,
        _user_org_id: Option<i64>,
    ) -> Result<EntityPermission, AccessError> {
        Ok(EntityPermission::AccessLevel {
            access_level: self.0,
        })
    }

    async fn get_crm_entity_permission_with_team(
        &self,
        _user_id: Option<&MacroUserId<Lowercase<'_>>>,
        _entity_id: &str,
        _entity_type: EntityType,
    ) -> Result<(EntityPermission, Uuid, TeamRole), AccessError> {
        unimplemented!("databases are not CRM entities")
    }

    async fn get_users_by_entity(
        &self,
        _entity_id: &str,
        _entity_type: EntityType,
    ) -> Result<Vec<MacroUserIdStr<'static>>, AccessError> {
        Ok(Vec::new())
    }

    async fn get_call_channel(
        &self,
        _call_id: &Uuid,
    ) -> Result<Option<CallChannelInfo>, AccessError> {
        Ok(None)
    }

    async fn get_call_channel_by_channel_id(
        &self,
        _channel_id: &Uuid,
    ) -> Result<Option<CallChannelInfo>, AccessError> {
        Ok(None)
    }

    async fn get_user_team(
        &self,
        _user_id: &MacroUserId<Lowercase<'_>>,
    ) -> Result<Option<UserTeamInfo>, AccessError> {
        Ok(None)
    }
}

/// Counts the batches that reached it; the ops route calls nothing else.
#[derive(Default)]
pub(super) struct RecordingService {
    pub(super) applied: Mutex<usize>,
}

const ONLY_OPS: &str = "the ops route calls only apply_ops";

impl DatabasesService for RecordingService {
    async fn apply_ops(
        &self,
        _receipt: EntityAccessReceipt<EditAccessLevel>,
        _viewer: Viewer,
        _ops: Vec<DatabaseOp>,
    ) -> Result<Vec<OpResult>, DatabaseError> {
        *self.applied.lock().unwrap() += 1;
        Ok(Vec::new())
    }

    async fn view_positions(
        &self,
        _: EntityAccessReceipt<ViewAccessLevel>,
        _: crate::domain::models::ViewId,
    ) -> Result<Vec<crate::domain::models::CardPosition>, DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }

    async fn create_database(&self, _: CreateDatabase) -> Result<Database, DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn list_databases(&self, _: Viewer) -> Result<Vec<ListedDatabase>, DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn database_details(&self, _: Viewer) -> Result<Vec<DatabaseDetail>, DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn get_database(
        &self,
        _: EntityAccessReceipt<ViewAccessLevel>,
        _: Viewer,
    ) -> Result<DatabaseDetail, DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn rename_database(
        &self,
        _: EntityAccessReceipt<EditAccessLevel>,
        _: String,
    ) -> Result<Database, DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn trash_database(
        &self,
        _: EntityAccessReceipt<OwnerAccessLevel>,
    ) -> Result<(), DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn restore_database(
        &self,
        _: EntityAccessReceipt<OwnerAccessLevel>,
    ) -> Result<(), DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn delete_database_permanently(
        &self,
        _: EntityAccessReceipt<OwnerAccessLevel>,
    ) -> Result<(), DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn create_table(
        &self,
        _: EntityAccessReceipt<EditAccessLevel>,
        _: CreateTable,
    ) -> Result<Table, DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn rename_table(
        &self,
        _: EntityAccessReceipt<EditAccessLevel>,
        _: TableId,
        _: String,
        _: String,
    ) -> Result<Table, DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn reorder_tables(
        &self,
        _: EntityAccessReceipt<EditAccessLevel>,
        _: Vec<TableId>,
    ) -> Result<Vec<Table>, DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn delete_table(
        &self,
        _: EntityAccessReceipt<EditAccessLevel>,
        _: TableId,
    ) -> Result<(), DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn create_column(
        &self,
        _: EntityAccessReceipt<EditAccessLevel>,
        _: Viewer,
        _: CreateColumn,
    ) -> Result<ColumnId, DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn rename_column(
        &self,
        _: EntityAccessReceipt<EditAccessLevel>,
        _: TableId,
        _: ColumnId,
        _: String,
        _: String,
    ) -> Result<RenameColumnOutcome, DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn infer_column_type(
        &self,
        _: EntityAccessReceipt<EditAccessLevel>,
        _: Viewer,
        _: InferColumnType,
    ) -> Result<InferColumnTypeOutcome, DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn change_column_type(
        &self,
        _: EntityAccessReceipt<EditAccessLevel>,
        _: Viewer,
        _: ChangeColumnType,
    ) -> Result<ColumnTypeChangeOutcome, DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn column_casts(
        &self,
        _: EntityAccessReceipt<ViewAccessLevel>,
        _: Viewer,
        _: TableId,
        _: ColumnId,
    ) -> Result<Vec<ColumnCast>, DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn delete_column(
        &self,
        _: EntityAccessReceipt<EditAccessLevel>,
        _: TableId,
        _: ColumnId,
        _: TableVersion,
    ) -> Result<ColumnSchemaOutcome, DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn reorder_columns(
        &self,
        _: EntityAccessReceipt<EditAccessLevel>,
        _: TableId,
        _: Vec<ColumnId>,
        _: TableVersion,
    ) -> Result<ColumnSchemaOutcome, DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn add_column_options(
        &self,
        _: EntityAccessReceipt<EditAccessLevel>,
        _: Viewer,
        _: AddColumnOptions,
    ) -> Result<ColumnDetail, DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn share_awareness(
        &self,
        _: EntityAccessReceipt<ViewAccessLevel>,
        _: Viewer,
        _: Awareness,
    ) -> Result<(), DatabaseError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn save_query(
        &self,
        _: Viewer,
        _: Option<DatabaseId>,
        _: QueryDefinition,
    ) -> Result<SavedQuery, SavedQueryError> {
        unimplemented!("{ONLY_OPS}")
    }
    async fn get_query(&self, _: Viewer, _: QueryId) -> Result<SavedQuery, SavedQueryError> {
        unimplemented!("{ONLY_OPS}")
    }
}
