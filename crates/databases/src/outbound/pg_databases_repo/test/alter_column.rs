//! The service as hosts build it, over a real Postgres: `ALTER COLUMN …
//! TYPE` through SQL, all the way to the stored cells.

use entity_access::domain::models::{
    AccessLevel, EditAccessLevel, Entity, EntityAccessReceipt, EntityPermission, EntityType,
};
use macro_db_migrator::MACRO_DB_MIGRATIONS;
use macro_event_broker::NoopMacroEventBroker;
use macro_user_id::{cowlike::CowLike, user_id::MacroUserIdStr};
use models_properties::shared::DataType;

use sqlx::PgPool;

use crate::domain::models::{
    AlteredColumn, ColumnBinding, CreateColumn, CreateDatabase, DatabaseId, ExecRequest,
    QueryError, SqlValue, Viewer,
};
use crate::domain::ports::{DatabasesRepo, DatabasesService};
use crate::outbound::build::{PgDatabasesService, build_service};
use crate::outbound::gateway_event_publisher::NoOpTableEventPublisher;
use crate::outbound::pg_databases_repo::PgDatabasesRepo;

const USER: &str = "macro|alter-column@macro.com";

fn viewer() -> Viewer {
    Viewer {
        user_id: MacroUserIdStr::parse_from_str(USER).unwrap().into_owned(),
        acting_bot: None,
    }
}

fn edit(database_id: DatabaseId) -> EntityAccessReceipt<EditAccessLevel> {
    EntityAccessReceipt::try_new_authenticated_user(
        MacroUserIdStr::parse_from_str(USER).unwrap().into_owned(),
        Entity {
            entity_id: database_id.to_string(),
            entity_type: EntityType::Database,
        },
        EntityPermission::AccessLevel {
            access_level: AccessLevel::Owner,
        },
    )
    .unwrap()
}

async fn insert_user(pool: &PgPool) {
    let id = macro_uuid::generate_uuid_v7();
    sqlx::query!(r#"INSERT INTO macro_user (id, username, email, stripe_customer_id) VALUES ($1, $2, $2, $2)"#, id, USER)
        .execute(pool).await.unwrap();
    sqlx::query!(
        r#"INSERT INTO "User" (id, email, macro_user_id) VALUES ($1, $1, $2)"#,
        USER,
        id
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn exec(
    service: &PgDatabasesService<NoOpTableEventPublisher, NoopMacroEventBroker>,
    sql: &str,
) -> Result<crate::domain::models::ExecOutcome, QueryError> {
    service
        .exec_sql(
            viewer(),
            ExecRequest {
                sql: sql.into(),
                scope: None,
                base_versions: None,
            },
        )
        .await
}

#[sqlx::test(migrator = "MACRO_DB_MIGRATIONS")]
async fn alter_column_type_refuses_misfits_then_clears_them_with_using_null(pool: PgPool) {
    insert_user(&pool).await;
    let service = build_service(pool.clone(), NoOpTableEventPublisher, NoopMacroEventBroker);
    let database = service
        .create_database(CreateDatabase {
            name: "Shop".into(),
            owner_id: viewer().user_id,
            acting_bot: None,
        })
        .await
        .unwrap();
    let table = PgDatabasesRepo::new(pool.clone())
        .get_database(database.id)
        .await
        .unwrap()
        .unwrap()
        .1[0]
        .clone();
    let price = service
        .create_column(
            edit(database.id),
            viewer(),
            CreateColumn {
                infer_type: false,
                table_id: table.id,
                binding: ColumnBinding::NewDefinition {
                    name: "Price".into(),
                    data_type: DataType::String,
                    is_multi_select: false,
                    options: vec![],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    exec(
        &service,
        "INSERT INTO \"Shop\".\"Table 1\" (\"Price\") VALUES ('12'), ('TBD')",
    )
    .await
    .unwrap();

    let refused = exec(
        &service,
        "ALTER TABLE \"Shop\".\"Table 1\" ALTER COLUMN \"Price\" TYPE number",
    )
    .await
    .unwrap_err();
    assert_eq!(
        refused.to_string(),
        "sql error: 1 value in \"Price\" isn't a number: 'TBD'. Fix it, or convert with \
         clearing to empty it."
    );

    let read_only = service
        .query_sql(
            viewer(),
            "ALTER TABLE \"Shop\".\"Table 1\" ALTER COLUMN \"Price\" TYPE number USING NULL".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(read_only, QueryError::ReadOnly(_)), "{read_only}");

    let altered = exec(
        &service,
        "ALTER TABLE \"Shop\".\"Table 1\" ALTER COLUMN \"Price\" TYPE number USING NULL",
    )
    .await
    .unwrap();
    assert_eq!(
        altered.altered_column,
        Some(AlteredColumn {
            table_id: table.id,
            column_id: price,
            name: "Price".into(),
            to: "number".into(),
            cleared_cells: 1,
            trimmed_cells: 0,
        })
    );
    let version = PgDatabasesRepo::new(pool.clone())
        .table_versions(&[table.id])
        .await
        .unwrap()[&table.id];
    assert_eq!(altered.new_versions[&table.id], version);

    let read = exec(
        &service,
        "SELECT \"Price\" FROM \"Shop\".\"Table 1\" ORDER BY \"Price\"",
    )
    .await
    .unwrap();
    let prices: Vec<&SqlValue> = read.results[0].rows.iter().map(|row| &row[1]).collect();
    assert_eq!(prices, vec![&SqlValue::Real(12.0), &SqlValue::Null]);
}
