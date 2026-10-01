//! Toolset tests over a fake service and entity access: receipts gate the
//! schema operations and errors reach the model in a form it can act on.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ai_toolset::schema::generate_validated_input_schema;
use ai_toolset::{AsyncTool, RequestContext, ServiceContext};
use chrono::Utc;
use entity_access::domain::models::{
    AccessError, AccessLevel, BotAccessScope, BotId, CallChannelInfo, Entity as AccessEntity,
    EntityAccessReceipt, EntityPermission, EntityType as AccessEntityType, OwnerAccessLevel,
    RequiredPermission, TeamRole, UserTeamInfo,
};
use macro_user_id::lowercased::Lowercase;
use macro_user_id::user_id::MacroUserId;
use models_properties::service::property_definition::PropertyDefinition;
use models_properties::service::property_definition_with_options::PropertyDefinitionWithOptions;
use models_properties::service::property_option::{PropertyOption, PropertyOptionValue};
use models_properties::shared::{DataType, PropertyOwner};
use uuid::Uuid;

use super::*;
mod committed_writes;
mod options;
mod receipts;
mod relations;
mod rendering;
mod schema_changes;
mod schemas;
mod views;
mod write_warning;
use crate::domain::models::{
    Column, ColumnDetail, Database, RenameColumnOutcome, Table, TableDetail, TableVersion,
};

const USER: &str = "macro|wolf@macro.com";

fn user() -> MacroUserIdStr<'static> {
    MacroUserIdStr::parse_from_str(USER).expect("valid user id")
}

fn request_context() -> RequestContext {
    RequestContext::new(user())
}

/// What the fake service was asked to do, so a test can assert a denied call
/// never reached it.
#[derive(Debug, Default, PartialEq)]
struct Calls {
    listed: usize,
    described: usize,
    created_databases: Vec<String>,
    created_tables: Vec<String>,
    renamed_tables: Vec<(Uuid, String, String)>,
    created_columns: Vec<(Uuid, DataType, bool, Vec<String>)>,
    added_options: Vec<(Uuid, Vec<String>)>,
    renamed_databases: Vec<String>,
    deleted_tables: Vec<Uuid>,
    /// `(table, column, name, previous name)`.
    renamed_columns: Vec<(Uuid, Uuid, String, String)>,
    changed_column_types: Vec<crate::domain::models::ChangeColumnType>,
    /// `(table, column, base version)`.
    deleted_columns: Vec<(Uuid, Uuid, TableVersion)>,
    /// `(table, order, base version)`.
    reordered_columns: Vec<(Uuid, Vec<Uuid>, TableVersion)>,
    /// Every table order the service was asked for.
    reordered_tables: Vec<Vec<Uuid>>,
    /// The agent each attributed write reached the service as.
    acting_bots: Vec<Option<BotId>>,
    /// Every batch of ops the service was asked to apply.
    applied: Vec<Vec<models_databases::DatabaseOp>>,
}

#[derive(Clone, Default)]
struct FakeService {
    calls: Arc<Mutex<Calls>>,
    /// Fail only the post-write schema enrichment.
    schema_error: bool,
    multi_select_group: bool,
    /// The described table's views.
    views: Vec<crate::domain::models::DatabaseView>,
}

const DATABASE_ID: Uuid = Uuid::from_u128(0x0dbb_0000_0000_0000_0000_0000_0000_0001);
const TABLE_ID: Uuid = Uuid::from_u128(0x7ab1_0000_0000_0000_0000_0000_0000_0001);
const COLUMN_ID: Uuid = Uuid::from_u128(0xc01a_0000_0000_0000_0000_0000_0000_0001);
const VIEW_ID: Uuid = Uuid::from_u128(0x71e0_0000_0000_0000_0000_0000_0000_0001);

fn database() -> Database {
    Database {
        id: DATABASE_ID,
        name: "Offsite".to_string(),
        owner_id: USER.to_string(),
        created_at: Utc::now(),
        trashed_at: None,
    }
}

fn table() -> Table {
    Table {
        id: TABLE_ID,
        database_id: DATABASE_ID,
        name: "Guests".to_string(),
        position: "a".to_string(),
        version: TableVersion(3),
    }
}

/// A select column with two options, which is what exercises option rendering.
fn status_column() -> ColumnDetail {
    ColumnDetail {
        column: Column {
            infer_type: false,
            display_name: None,
            id: COLUMN_ID,
            table_id: TABLE_ID,
            property_definition_id: Uuid::nil(),
            position: "a".to_string(),
            config: None,
        },
        sql_name: "\"Status\"".to_string(),
        definition: PropertyDefinitionWithOptions {
            definition: PropertyDefinition {
                id: Uuid::nil(),
                owner: PropertyOwner::Database {
                    database_id: DATABASE_ID,
                },
                display_name: "Status".to_string(),
                data_type: DataType::SelectString,
                is_multi_select: false,
                specific_entity_type: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                is_system: false,
                is_metadata: false,
            },
            property_options: vec![option("Going", 0), option("Declined", 1)],
        },
        writable: true,
        shared_outside_database: false,
    }
}

fn option(label: &str, display_order: i32) -> PropertyOption {
    PropertyOption {
        id: Uuid::new_v4(),
        property_definition_id: Uuid::nil(),
        display_order,
        value: PropertyOptionValue::String(label.to_string()),
        color: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn detail(grant: AccessLevel) -> DatabaseDetail {
    DatabaseDetail {
        database: database(),
        grant,
        tables: vec![TableDetail {
            table: table(),
            sql_name: "\"Offsite\".\"Guests\"".to_string(),
            read_sql_name: "\"Offsite\".\"Guests\"".to_string(),
            columns: vec![status_column()],
            views: vec![],
        }],
    }
}

impl DatabasesService for FakeService {
    async fn create_database(
        &self,
        cmd: crate::domain::models::CreateDatabase,
    ) -> Result<Database, DatabaseError> {
        let mut calls = self.calls.lock().unwrap();
        calls.created_databases.push(cmd.name);
        calls.acting_bots.push(cmd.acting_bot);
        Ok(database())
    }

    async fn list_databases(&self, _viewer: Viewer) -> Result<Vec<ListedDatabase>, DatabaseError> {
        self.calls.lock().unwrap().listed += 1;
        Ok(vec![ListedDatabase {
            database: database(),
            grant: AccessLevel::Owner,
            tables: vec![table()],
        }])
    }

    async fn database_details(
        &self,
        _viewer: Viewer,
    ) -> Result<Vec<DatabaseDetail>, DatabaseError> {
        unimplemented!("no tool reads every database in detail")
    }

    async fn share_awareness(
        &self,
        _receipt: EntityAccessReceipt<ViewAccessLevel>,
        _viewer: Viewer,
        _state: crate::domain::models::Awareness,
    ) -> Result<(), DatabaseError> {
        unimplemented!("the toolset does not share awareness")
    }

    async fn view_positions(
        &self,
        _receipt: EntityAccessReceipt<ViewAccessLevel>,
        _view_id: crate::domain::models::ViewId,
    ) -> Result<Vec<crate::domain::models::CardPosition>, DatabaseError> {
        unimplemented!("the toolset does not read card places")
    }

    /// Answers a view op with the view it would leave, as of a fixed time.
    async fn apply_ops(
        &self,
        _receipt: EntityAccessReceipt<EditAccessLevel>,
        _viewer: Viewer,
        ops: Vec<models_databases::DatabaseOp>,
    ) -> Result<Vec<models_databases::OpResult>, DatabaseError> {
        use models_databases::{DatabaseOp, OpResult};
        self.calls.lock().unwrap().applied.push(ops.clone());
        let at = chrono::DateTime::UNIX_EPOCH;
        Ok(ops
            .into_iter()
            .map(|op| {
                let view = match op {
                    DatabaseOp::CreateView { table, view } => crate::domain::models::DatabaseView {
                        id: VIEW_ID,
                        database_id: DATABASE_ID,
                        table_id: table,
                        name: view.name,
                        position: "80".into(),
                        query: view.query,
                        layout: view.layout,
                        created_at: at,
                        updated_at: at,
                    },
                    DatabaseOp::UpdateView {
                        view,
                        name,
                        query,
                        layout,
                        ..
                    } => {
                        let current = self
                            .views
                            .iter()
                            .find(|stored| stored.id == view)
                            .expect("the view the tool found")
                            .clone();
                        crate::domain::models::DatabaseView {
                            name: name.unwrap_or(current.name),
                            query: query.unwrap_or(current.query),
                            layout: layout.unwrap_or(current.layout),
                            ..current
                        }
                    }
                    other => unimplemented!("the toolset sends no {other:?}"),
                };
                OpResult::ViewWritten {
                    table_version: TableVersion(4),
                    view: Box::new(view),
                }
            })
            .collect())
    }

    async fn get_database(
        &self,
        _receipt: EntityAccessReceipt<ViewAccessLevel>,
        _viewer: Viewer,
    ) -> Result<DatabaseDetail, DatabaseError> {
        self.calls.lock().unwrap().described += 1;
        if self.schema_error {
            return Err(DatabaseError::Repo(
                rootcause::Report::new(std::io::Error::other("schema connection lost"))
                    .into_dynamic(),
            ));
        }
        let mut database = detail(AccessLevel::Owner);
        database.tables[0].columns[0]
            .definition
            .definition
            .is_multi_select = self.multi_select_group;
        database.tables[0].views = self.views.clone();
        Ok(database)
    }

    async fn rename_database(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        name: String,
    ) -> Result<Database, DatabaseError> {
        let mut calls = self.calls.lock().unwrap();
        calls.renamed_databases.push(name.clone());
        calls.acting_bots.push(match receipt.auth() {
            entity_access::domain::models::EntityAccessAuth::Bot(bot) => Some(bot.bot_id()),
            _ => None,
        });
        Ok(Database { name, ..database() })
    }

    async fn delete_table(
        &self,
        _receipt: EntityAccessReceipt<EditAccessLevel>,
        table_id: crate::domain::models::TableId,
    ) -> Result<(), DatabaseError> {
        self.calls.lock().unwrap().deleted_tables.push(table_id);
        Ok(())
    }

    async fn rename_table(
        &self,
        _receipt: EntityAccessReceipt<EditAccessLevel>,
        table_id: crate::domain::models::TableId,
        name: String,
        previous_name: String,
    ) -> Result<Table, DatabaseError> {
        self.calls
            .lock()
            .unwrap()
            .renamed_tables
            .push((table_id, name.clone(), previous_name));
        Ok(Table { name, ..table() })
    }

    async fn reorder_tables(
        &self,
        _receipt: EntityAccessReceipt<EditAccessLevel>,
        table_ids: Vec<crate::domain::models::TableId>,
    ) -> Result<Vec<Table>, DatabaseError> {
        self.calls
            .lock()
            .unwrap()
            .reordered_tables
            .push(table_ids.clone());
        Ok(table_ids
            .into_iter()
            .map(|id| Table { id, ..table() })
            .collect())
    }

    async fn infer_column_type(
        &self,
        _: EntityAccessReceipt<EditAccessLevel>,
        _: Viewer,
        _: crate::domain::models::InferColumnType,
    ) -> Result<crate::domain::models::InferColumnTypeOutcome, DatabaseError> {
        unimplemented!("tool tests do not infer column types")
    }
    async fn change_column_type(
        &self,
        _: EntityAccessReceipt<EditAccessLevel>,
        _: Viewer,
        cmd: crate::domain::models::ChangeColumnType,
    ) -> Result<crate::domain::models::ColumnTypeChangeOutcome, DatabaseError> {
        let table_id = cmd.table_id;
        let cleared_cells = if cmd.clear_invalid { 2 } else { 0 };
        self.calls.lock().unwrap().changed_column_types.push(cmd);
        Ok(crate::domain::models::ColumnTypeChangeOutcome {
            table_versions: HashMap::from([(table_id, TableVersion(4))]),
            cleared_cells,
            trimmed_cells: 0,
        })
    }
    async fn column_casts(
        &self,
        _: EntityAccessReceipt<ViewAccessLevel>,
        _: Viewer,
        _: Uuid,
        _: Uuid,
    ) -> Result<Vec<crate::domain::models::ColumnCast>, DatabaseError> {
        unimplemented!("tool tests do not preview type changes")
    }
    async fn delete_column(
        &self,
        _: EntityAccessReceipt<EditAccessLevel>,
        table_id: Uuid,
        column_id: Uuid,
        base_version: TableVersion,
    ) -> Result<crate::domain::models::ColumnSchemaOutcome, DatabaseError> {
        self.calls
            .lock()
            .unwrap()
            .deleted_columns
            .push((table_id, column_id, base_version));
        Ok(crate::domain::models::ColumnSchemaOutcome {
            table_versions: HashMap::from([(table_id, TableVersion(4))]),
        })
    }
    async fn reorder_columns(
        &self,
        _: EntityAccessReceipt<EditAccessLevel>,
        table_id: Uuid,
        column_ids: Vec<Uuid>,
        base_version: TableVersion,
    ) -> Result<crate::domain::models::ColumnSchemaOutcome, DatabaseError> {
        self.calls
            .lock()
            .unwrap()
            .reordered_columns
            .push((table_id, column_ids, base_version));
        Ok(crate::domain::models::ColumnSchemaOutcome {
            table_versions: HashMap::from([(table_id, TableVersion(4))]),
        })
    }
    async fn rename_column(
        &self,
        _receipt: EntityAccessReceipt<EditAccessLevel>,
        table_id: crate::domain::models::TableId,
        column_id: crate::domain::models::ColumnId,
        name: String,
        previous_name: String,
    ) -> Result<RenameColumnOutcome, DatabaseError> {
        self.calls.lock().unwrap().renamed_columns.push((
            table_id,
            column_id,
            name.clone(),
            previous_name,
        ));
        let mut column = status_column().column;
        column.display_name = Some(name.trim().to_string());
        Ok(RenameColumnOutcome {
            column,
            table_version: TableVersion(4),
        })
    }

    async fn trash_database(
        &self,
        _receipt: EntityAccessReceipt<OwnerAccessLevel>,
    ) -> Result<(), DatabaseError> {
        unimplemented!("the toolset does not trash databases")
    }

    async fn restore_database(
        &self,
        _receipt: EntityAccessReceipt<OwnerAccessLevel>,
    ) -> Result<(), DatabaseError> {
        unimplemented!("the toolset does not restore databases")
    }

    async fn delete_database_permanently(
        &self,
        _receipt: EntityAccessReceipt<OwnerAccessLevel>,
    ) -> Result<(), DatabaseError> {
        unimplemented!("the toolset does not delete databases")
    }

    async fn create_table(
        &self,
        _receipt: EntityAccessReceipt<EditAccessLevel>,
        cmd: crate::domain::models::CreateTable,
    ) -> Result<Table, DatabaseError> {
        self.calls.lock().unwrap().created_tables.push(cmd.name);
        Ok(table())
    }

    async fn create_column(
        &self,
        _receipt: EntityAccessReceipt<EditAccessLevel>,
        _viewer: Viewer,
        cmd: crate::domain::models::CreateColumn,
    ) -> Result<crate::domain::models::ColumnId, DatabaseError> {
        let crate::domain::models::ColumnBinding::NewDefinition {
            data_type,
            is_multi_select,
            options,
            ..
        } = cmd.binding
        else {
            panic!("the tool only ever creates fresh definitions");
        };
        self.calls.lock().unwrap().created_columns.push((
            cmd.table_id,
            data_type,
            is_multi_select,
            options,
        ));
        Ok(COLUMN_ID)
    }

    async fn add_column_options(
        &self,
        _receipt: EntityAccessReceipt<EditAccessLevel>,
        _viewer: Viewer,
        cmd: crate::domain::models::AddColumnOptions,
    ) -> Result<ColumnDetail, DatabaseError> {
        self.calls
            .lock()
            .unwrap()
            .added_options
            .push((cmd.column_id, cmd.labels.clone()));
        let mut column = status_column();
        for (offset, label) in cmd.labels.iter().enumerate() {
            column
                .definition
                .property_options
                .push(option(label, 2 + offset as i32));
        }
        Ok(column)
    }

    async fn save_query(
        &self,
        _viewer: Viewer,
        _database_id: Option<crate::domain::models::DatabaseId>,
        _definition: crate::domain::models::QueryDefinition,
    ) -> Result<crate::domain::models::SavedQuery, crate::domain::models::SavedQueryError> {
        unimplemented!("no tool saves a query")
    }

    async fn get_query(
        &self,
        _viewer: Viewer,
        _id: crate::domain::models::QueryId,
    ) -> Result<crate::domain::models::SavedQuery, crate::domain::models::SavedQueryError> {
        unimplemented!("no tool reads a saved query back")
    }
}

/// Grants exactly `level`, or nothing at all when `level` is `None`.
#[derive(Clone)]
struct FakeAccess {
    level: Option<AccessLevel>,
}

impl FakeAccess {
    fn granting(level: AccessLevel) -> Arc<Self> {
        Arc::new(Self { level: Some(level) })
    }

    fn denying() -> Arc<Self> {
        Arc::new(Self { level: None })
    }
}

impl EntityAccessService for FakeAccess {
    async fn generate_entity_access_receipt<T: RequiredPermission>(
        &self,
        _user_id: &MacroUserId<Lowercase<'_>>,
        _user_org_id: Option<i64>,
        entity_id: &str,
        entity_type: AccessEntityType,
    ) -> Result<EntityAccessReceipt<T>, AccessError> {
        let Some(access_level) = self.level else {
            return Err(AccessError::Unauthorized);
        };
        EntityAccessReceipt::try_new_authenticated_user(
            user(),
            AccessEntity {
                entity_id: entity_id.to_string(),
                entity_type,
            },
            EntityPermission::AccessLevel { access_level },
        )
    }

    async fn generate_bot_entity_access_receipt<T: RequiredPermission>(
        &self,
        bot_id: BotId,
        scope: BotAccessScope,
        entity_id: &str,
        entity_type: AccessEntityType,
    ) -> Result<EntityAccessReceipt<T>, AccessError> {
        let Some(access_level) = self.level else {
            return Err(AccessError::Unauthorized);
        };
        EntityAccessReceipt::try_new_bot(
            bot_id.into_storage_id(),
            (&scope).into(),
            AccessEntity {
                entity_id: entity_id.to_string(),
                entity_type,
            },
            EntityPermission::AccessLevel { access_level },
        )
    }

    async fn get_access_level(
        &self,
        _user_id: Option<&MacroUserId<Lowercase<'_>>>,
        _entity_id: &str,
        _entity_type: AccessEntityType,
    ) -> Result<Option<AccessLevel>, AccessError> {
        Ok(self.level)
    }

    async fn check_access(
        &self,
        _user_id: Option<&MacroUserId<Lowercase<'_>>>,
        _entity_id: &str,
        _entity_type: AccessEntityType,
        _required_level: AccessLevel,
    ) -> Result<AccessLevel, AccessError> {
        self.level.ok_or(AccessError::Unauthorized)
    }

    async fn check_public_access(
        &self,
        _entity_id: &str,
        _entity_type: AccessEntityType,
        _required_level: AccessLevel,
    ) -> Result<AccessLevel, AccessError> {
        Err(AccessError::Unauthorized)
    }

    async fn get_entity_permission(
        &self,
        _user_id: Option<&MacroUserId<Lowercase<'_>>>,
        _entity_id: &str,
        _entity_type: AccessEntityType,
        _user_org_id: Option<i64>,
    ) -> Result<EntityPermission, AccessError> {
        self.level
            .map(|access_level| EntityPermission::AccessLevel { access_level })
            .ok_or(AccessError::Unauthorized)
    }

    async fn get_crm_entity_permission_with_team(
        &self,
        _user_id: Option<&MacroUserId<Lowercase<'_>>>,
        _entity_id: &str,
        _entity_type: AccessEntityType,
    ) -> Result<(EntityPermission, Uuid, TeamRole), AccessError> {
        unimplemented!("databases tools never touch CRM entities")
    }

    async fn get_users_by_entity(
        &self,
        _entity_id: &str,
        _entity_type: AccessEntityType,
    ) -> Result<Vec<MacroUserIdStr<'static>>, AccessError> {
        Ok(vec![user()])
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

type Context = DatabasesToolContext<FakeService, FakeAccess>;

fn context(access: Arc<FakeAccess>) -> (Context, Arc<Mutex<Calls>>) {
    let service = FakeService::default();
    let calls = service.calls.clone();
    (DatabasesToolContext::new(service, access), calls)
}
