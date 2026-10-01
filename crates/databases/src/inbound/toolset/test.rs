//! Toolset tests: fake service + fake entity access, asserting that the
//! receipts gate the schema operations and that errors reach the model in a
//! form it can act on.

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
mod relations;
mod schema_changes;
mod views;
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

// --- fakes ---

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
        column.display_name = Some(name);
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

// --- schema validation ---

#[test]
fn every_tool_schema_is_valid() {
    assert_eq!(
        generate_validated_input_schema::<SaveDatabaseView>()
            .expect("view schema validates")
            .name,
        "SaveDatabaseView"
    );
    assert_eq!(
        generate_validated_input_schema::<ListDatabases>()
            .expect("schema should validate")
            .name,
        "ListDatabases"
    );
    assert_eq!(
        generate_validated_input_schema::<DescribeDatabase>()
            .expect("schema should validate")
            .name,
        "DescribeDatabase"
    );
    assert_eq!(
        generate_validated_input_schema::<CreateDatabase>()
            .expect("schema should validate")
            .name,
        "CreateDatabase"
    );
    assert_eq!(
        generate_validated_input_schema::<CreateTable>()
            .expect("schema should validate")
            .name,
        "CreateTable"
    );
    assert_eq!(
        generate_validated_input_schema::<RenameTable>()
            .expect("schema should validate")
            .name,
        "RenameTable"
    );
    assert_eq!(
        generate_validated_input_schema::<AddColumn>()
            .expect("schema should validate")
            .name,
        "AddColumn"
    );
    assert_eq!(
        generate_validated_input_schema::<AddColumnOptions>()
            .expect("schema should validate")
            .name,
        "AddColumnOptions"
    );
    assert_eq!(
        generate_validated_input_schema::<RenameDatabase>()
            .expect("schema should validate")
            .name,
        "RenameDatabase"
    );
    assert_eq!(
        generate_validated_input_schema::<DeleteTable>()
            .expect("schema should validate")
            .name,
        "DeleteTable"
    );
    assert_eq!(
        generate_validated_input_schema::<RenameColumn>()
            .expect("schema should validate")
            .name,
        "RenameColumn"
    );
    assert_eq!(
        generate_validated_input_schema::<ChangeColumnType>()
            .expect("schema should validate")
            .name,
        "ChangeColumnType"
    );
    assert_eq!(
        generate_validated_input_schema::<DeleteColumn>()
            .expect("schema should validate")
            .name,
        "DeleteColumn"
    );
    assert_eq!(
        generate_validated_input_schema::<ReorderColumns>()
            .expect("schema should validate")
            .name,
        "ReorderColumns"
    );
    assert_eq!(
        generate_validated_input_schema::<ReorderTables>()
            .expect("schema should validate")
            .name,
        "ReorderTables"
    );
}

/// Every tool has to survive being put in a collection — that is where name
/// conflicts and schema rejections actually surface.
#[test]
fn toolset_builds_with_every_tool() {
    let toolset = databases_toolset::<FakeService, FakeAccess>();

    for name in [
        "ListDatabases",
        "DescribeDatabase",
        "CreateDatabase",
        "CreateTable",
        "RenameDatabase",
        "RenameTable",
        "ReorderTables",
        "DeleteTable",
        "AddColumn",
        "AddColumnOptions",
        "RenameColumn",
        "ChangeColumnType",
        "DeleteColumn",
        "ReorderColumns",
        "SaveDatabaseView",
    ] {
        assert!(toolset.tools.contains_key(name), "missing {name}");
    }
    assert_eq!(toolset.tools.len(), 15);
    assert!(
        toolset.user_tools.is_empty(),
        "database tools run in the loop, none are user-executed"
    );
}

#[test]
fn the_read_only_toolset_only_discovers() {
    let toolset = databases_read_only_toolset::<FakeService, FakeAccess>();

    let mut names: Vec<&str> = toolset.tools.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(names, ["DescribeDatabase", "ListDatabases"]);
}

// --- receipts gate the schema operations ---

#[tokio::test]
async fn describe_needs_a_view_receipt() {
    let (context, calls) = context(FakeAccess::denying());
    let error = DescribeDatabase {
        database_id: DATABASE_ID,
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect_err("no access means no schema");

    assert!(
        error
            .description
            .contains("does not have permission to read"),
        "{}",
        error.description
    );
    assert_eq!(
        calls.lock().unwrap().described,
        0,
        "the service must not be reached without a receipt"
    );
}

/// View access reads; it does not create tables. The receipt type is what
/// draws that line, and it is drawn before the service is touched.
#[tokio::test]
async fn creating_a_table_needs_more_than_view_access() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::View));
    let error = CreateTable {
        database_id: DATABASE_ID,
        name: "Sessions".to_string(),
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect_err("view access cannot change the schema");

    assert!(
        error
            .description
            .contains("does not have permission to edit"),
        "{}",
        error.description
    );
    assert!(calls.lock().unwrap().created_tables.is_empty());
}

#[tokio::test]
async fn renaming_a_table_needs_more_than_view_access() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::View));
    let error = RenameTable {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        name: "Attendees".to_string(),
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect_err("view access cannot rename a tab");

    assert!(
        error
            .description
            .contains("does not have permission to edit"),
        "{}",
        error.description
    );
    assert!(calls.lock().unwrap().renamed_tables.is_empty());
}

/// The service's compare-and-swap needs the name being replaced; the tool
/// supplies the current one rather than asking the model to repeat it.
#[tokio::test]
async fn renaming_a_table_replaces_its_current_name() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let response = RenameTable {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        name: "Attendees".to_string(),
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect("edit access may rename a tab");

    assert_eq!(response.table_id, TABLE_ID);
    assert_eq!(response.name, "Attendees");
    assert!(response.database.is_some());
    assert_eq!(
        calls.lock().unwrap().renamed_tables,
        vec![(TABLE_ID, "Attendees".to_string(), "Guests".to_string())]
    );
}

#[tokio::test]
async fn renaming_an_unknown_table_points_at_describe() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let error = RenameTable {
        database_id: DATABASE_ID,
        table_id: Uuid::nil(),
        name: "Attendees".to_string(),
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect_err("the table is not in this database");

    assert!(
        error.description.contains("DescribeDatabase"),
        "{}",
        error.description
    );
    assert!(calls.lock().unwrap().renamed_tables.is_empty());
}

#[tokio::test]
async fn creating_a_table_with_edit_access_succeeds() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let response = CreateTable {
        database_id: DATABASE_ID,
        name: "Sessions".to_string(),
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect("edit access may add a tab");

    assert_eq!(response.table_id, TABLE_ID);
    assert_eq!(calls.lock().unwrap().created_tables, vec!["Sessions"]);
    assert_eq!(
        response.database.expect("schema refresh succeeds").tables[0].sql_name,
        "\"Offsite\".\"Guests\"",
        "the SQL name is the display name, quoted"
    );
}

#[tokio::test]
async fn adding_a_column_needs_more_than_view_access() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::View));
    let error = AddColumn {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        name: "Dietary Needs".to_string(),
        data_type: ColumnType::Select,
        is_multi_select: true,
        options: Some(vec!["Vegan".to_string()]),
        link_to_table_id: None,
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect_err("view access cannot change the schema");

    assert!(
        error
            .description
            .contains("does not have permission to edit"),
        "{}",
        error.description
    );
    assert!(calls.lock().unwrap().created_columns.is_empty());
}

/// The tool's vocabulary has to reach the property system unchanged, or a
/// column is created as one type and read back as another.
#[tokio::test]
async fn adding_a_column_passes_the_type_through() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let response = AddColumn {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        name: "Dietary Needs".to_string(),
        data_type: ColumnType::Select,
        is_multi_select: true,
        options: Some(vec!["Vegan".to_string(), "Gluten-free".to_string()]),
        link_to_table_id: None,
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect("edit access may add a column");

    assert_eq!(response.column_id, COLUMN_ID);
    assert_eq!(
        calls.lock().unwrap().created_columns,
        vec![(
            TABLE_ID,
            DataType::SelectString,
            true,
            vec!["Vegan".to_string(), "Gluten-free".to_string()]
        )]
    );
}

#[test]
fn column_types_round_trip_through_the_property_system() {
    for column_type in [
        ColumnType::Text,
        ColumnType::Number,
        ColumnType::Boolean,
        ColumnType::Date,
        ColumnType::Link,
        ColumnType::Select,
        ColumnType::SelectNumber,
        ColumnType::Tag,
        ColumnType::Entity,
    ] {
        let stored: DataType = column_type.into();
        assert_eq!(
            ColumnType::from(stored),
            column_type,
            "{column_type:?} did not round trip"
        );
    }
}

// --- rendering ---

#[tokio::test]
async fn listing_renders_the_grant() {
    let (context, _) = context(FakeAccess::granting(AccessLevel::Owner));
    let response = ListDatabases {}
        .call(ServiceContext(context), request_context())
        .await
        .expect("listing needs no receipt");

    assert_eq!(response.databases.len(), 1);
    assert_eq!(response.databases[0].grant, ToolGrant::Owner);
    assert_eq!(response.databases[0].name, "Offsite");
    assert_eq!(response.databases[0].tables[0].name, "Guests");
    assert_eq!(response.databases[0].tables[0].id, TABLE_ID);
    assert_eq!(
        response.databases[0].tables[0].sql_name,
        "\"Offsite\".\"Guests\""
    );
    assert_eq!(response.summary, "Found 1 database.");
}

#[test]
fn an_empty_list_says_so_rather_than_looking_like_a_failure() {
    assert!(list_databases::summarize(&[]).contains("No accessible databases"));
}

/// Select options reach the model as the labels SQL accepts, and with the
/// ids a view names them by.
#[test]
fn describing_a_database_renders_option_labels() {
    let schema = ToolDatabaseSchema::from(detail(AccessLevel::Owner));

    assert_eq!(schema.tables[0].sql_name, "\"Offsite\".\"Guests\"");
    assert_eq!(schema.tables[0].version, 3);
    assert!(schema.tables[0].writable);
    let column = &schema.tables[0].columns[0];
    assert_eq!(column.sql_name, "\"Status\"");
    assert_eq!(column.data_type, ColumnType::Select);
    let labels: Vec<&str> = column
        .options
        .iter()
        .map(|option| option.label.as_str())
        .collect();
    assert_eq!(labels, vec!["Going", "Declined"]);
    let detail = detail(AccessLevel::Owner);
    let ids: Vec<Uuid> = column.options.iter().map(|option| option.id).collect();
    let stored: Vec<Uuid> = detail.tables[0].columns[0]
        .definition
        .property_options
        .iter()
        .map(|option| option.id)
        .collect();
    assert_eq!(ids.len(), stored.len());
}

#[test]
fn describing_a_renamed_column_supplies_its_current_label_as_the_sql_identifier() {
    let mut database = detail(AccessLevel::Owner);
    database.tables[0].columns[0].column.display_name = Some("RSVP".into());
    database.tables[0].columns[0].sql_name = "\"RSVP\"".into();
    let schema = ToolDatabaseSchema::from(database);
    let column = &schema.tables[0].columns[0];
    assert_eq!(column.name, "RSVP");
    assert_eq!(column.sql_name, "\"RSVP\"");
    let labels: Vec<&str> = column
        .options
        .iter()
        .map(|option| option.label.as_str())
        .collect();
    assert_eq!(labels, vec!["Going", "Declined"]);
}

#[test]
fn describing_an_entity_column_preserves_the_actual_entity_kind() {
    let mut database = detail(AccessLevel::Owner);
    let definition = &mut database.tables[0].columns[0].definition.definition;
    definition.data_type = DataType::Entity;
    definition.specific_entity_type = Some(models_properties::shared::EntityType::User);
    let json = serde_json::to_value(ToolDatabaseSchema::from(database)).unwrap();
    assert_eq!(
        json["tables"][0]["columns"][0]["specificEntityType"],
        "USER"
    );
    assert_eq!(json["tables"][0]["columns"][0]["dataType"], "entity");
}

/// A view-only grant has to read as unwritable, or the model writes an UPDATE
/// that the executor rejects after the user has been promised an edit.
#[test]
fn a_view_only_database_reads_as_unwritable() {
    let schema = ToolDatabaseSchema::from(detail(AccessLevel::View));
    assert_eq!(schema.grant, ToolGrant::View);
    assert!(!schema.tables[0].writable);
}

#[test]
fn the_response_serializes_with_camel_case_keys() {
    let schema = ToolDatabaseSchema::from(detail(AccessLevel::Owner));
    let json = serde_json::to_value(&schema).expect("schema should serialize");

    assert!(json["tables"][0]["sqlName"].is_string());
    assert!(json["tables"][0]["columns"][0]["isMultiSelect"].is_boolean());
}

// --- select options are explicit schema ---

/// The description is what stops a model creating an optionless select column
/// and then failing every INSERT against it.
#[test]
fn add_column_teaches_that_options_are_explicit() {
    let validated = generate_validated_input_schema::<AddColumn>().expect("schema should validate");

    assert!(
        validated.description.contains("explicit schema"),
        "{}",
        validated.description
    );
    assert!(
        validated.description.contains("AddColumnOptions"),
        "the description must point at the way to add more: {}",
        validated.description
    );
}

#[tokio::test]
async fn adding_options_needs_more_than_view_access() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::View));
    let error = AddColumnOptions {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        column_id: COLUMN_ID,
        labels: vec!["Waitlisted".to_string()],
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect_err("view access cannot change the schema");

    assert!(
        error
            .description
            .contains("does not have permission to edit"),
        "{}",
        error.description
    );
    assert!(calls.lock().unwrap().added_options.is_empty());
}

/// The response carries the labels SQL now accepts, so the model can write the
/// statement that just failed without describing the database again.
#[tokio::test]
async fn adding_options_returns_the_labels_sql_accepts() {
    let (context, calls) = context(FakeAccess::granting(AccessLevel::Edit));
    let response = AddColumnOptions {
        database_id: DATABASE_ID,
        table_id: TABLE_ID,
        column_id: COLUMN_ID,
        labels: vec!["Waitlisted".to_string()],
    }
    .call(ServiceContext(context), request_context())
    .await
    .expect("edit access may extend a select column");

    assert_eq!(response.column_id, COLUMN_ID);
    assert_eq!(response.options, vec!["Going", "Declined", "Waitlisted"]);
    assert_eq!(
        calls.lock().unwrap().added_options,
        vec![(COLUMN_ID, vec!["Waitlisted".to_string()])]
    );
}

#[test]
fn describe_column_schema_accepts_omitted_select_options() {
    let schema = serde_json::to_value(schemars::schema_for!(ToolColumn)).unwrap();
    let required = schema["required"].as_array().unwrap();
    assert!(
        !required.contains(&serde_json::json!("options")),
        "non-select columns omit empty options in actual tool responses"
    );
    assert!(required.contains(&serde_json::json!("name")));
}
