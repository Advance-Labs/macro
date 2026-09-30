use crate::properties::augment_authoritative;
use cache_core::{
    engine::{Engine, NetworkWrite},
    predicate::PredicateIndexStorage,
};
use serde_json::{Map, Value, json};

const QUERY: &str = include_str!("test/rows.graphql");
const VIEWER: &str = "macro|viewer@databases.test";
const DEALS: &str = "7ab00000-0000-0000-0000-000000000001";
const CONTACTS: &str = "7ab00000-0000-0000-0000-000000000002";
const STAGE: &str = "5e1ec700-0000-0000-0000-000000000001";
const WON: &str = "0e000000-0000-0000-0000-000000000001";
const LOST: &str = "0e000000-0000-0000-0000-000000000002";
const DEAL_1: &str = "70000000-0000-0000-0000-000000000001";
const DEAL_2: &str = "70000000-0000-0000-0000-000000000002";
const DEAL_3: &str = "70000000-0000-0000-0000-000000000003";
const CONTACT: &str = "70000000-0000-0000-0000-000000000004";

fn row(id: &str, table: &str, created_at: &str, stage: Option<(&str, &str)>) -> Value {
    json!({
        "__typename": "GraphqlSoupDatabaseRow", "id": id, "isFavorited": false,
        "cacheProjection": null, "notifications": [],
        "tableId": table, "databaseId": "db000000-0000-0000-0000-000000000001",
        "ownerId": "macro|owner@databases.test",
        "createdAt": created_at, "updatedAt": created_at,
        "properties": stage.map_or_else(Vec::new, |(property, option)| vec![json!({
            "id": property, "propertyDefinitionId": STAGE,
            "value": { "__typename": "GraphqlSelectOptionPropertyValue", "optionIds": [option] }
        })]),
    })
}

fn page(rows: Vec<Value>) -> Value {
    json!({ "user": { "id": VIEWER, "soup": { "items": rows } } })
}

/// Rows of one table, every other kind ruled out the way the browser does.
fn rows_of(table: &str, properties: Option<Value>) -> Value {
    let nil = "00000000-0000-0000-0000-000000000000";
    let mut filters = json!({
        "databaseRowFilter": { "literal": { "tableId": table } },
        "documentFilter": { "literal": { "id": nil } },
        "projectFilter": { "literal": { "projectIdSelf": nil } },
        "chatFilter": { "literal": { "chatId": nil } },
        "emailFilter": { "tree": { "literal": { "threadId": nil } } },
        "channelFilter": { "literal": { "channelId": nil } },
        "channelThreadFilter": { "literal": { "threadId": nil } },
        "calendarEventFilter": { "literal": { "id": nil } },
        "callFilter": { "literal": { "callId": nil } },
        "crmCompanyFilter": { "literal": { "id": nil } },
        "foreignEntityFilter": { "literal": { "id": nil } },
    });
    if let Some(properties) = properties {
        filters["propertiesFilter"] = properties;
    }
    filters
}

async fn write<S: PredicateIndexStorage>(engine: &mut Engine<S>, data: &Value) {
    let core = crate::authoritative_projection_mutations(QUERY, None, data).unwrap();
    let projections =
        augment_authoritative(engine.storage(), QUERY, None, &Map::new(), data, true, core)
            .await
            .unwrap();
    engine
        .write_query_with_registration_and_projections(
            None,
            None,
            NetworkWrite {
                query: QUERY,
                operation_name: None,
                variables: &Map::new(),
                data,
                identity: Some(VIEWER),
            },
            projections,
        )
        .await
        .unwrap();
}

async fn local_rows<S: PredicateIndexStorage>(
    engine: &mut Engine<S>,
    filters: Value,
) -> Vec<String> {
    let crate::SoupFilterCompileOutcome::Supported(query) =
        crate::compile_current_filter_request(filters, "CREATED_AT", "DESC", 100).unwrap()
    else {
        panic!("a table's rows compile to the local index")
    };
    engine
        .reconcile_predicate_index(&query, &[])
        .await
        .unwrap()
        .value
        .keys
        .into_iter()
        .map(|key| key.as_str().to_owned())
        .collect()
}

#[test]
fn a_tables_rows_are_answered_from_the_cache_and_pick_up_a_new_row() {
    pollster::block_on(async {
        let mut engine = Engine::new(cache_turso::TursoStorage::open_in_memory("rows").unwrap());
        write(
            &mut engine,
            &page(vec![
                row(
                    DEAL_1,
                    DEALS,
                    "2026-01-01T00:00:00Z",
                    Some(("e0000000-0000-0000-0000-000000000001", WON)),
                ),
                row(
                    DEAL_2,
                    DEALS,
                    "2026-01-02T00:00:00Z",
                    Some(("e0000000-0000-0000-0000-000000000002", LOST)),
                ),
                row(CONTACT, CONTACTS, "2026-01-04T00:00:00Z", None),
            ]),
        )
        .await;

        assert_eq!(
            local_rows(&mut engine, rows_of(DEALS, None)).await,
            vec![
                format!("GraphqlSoupDatabaseRow:{DEAL_2}"),
                format!("GraphqlSoupDatabaseRow:{DEAL_1}"),
            ]
        );
        assert_eq!(
            local_rows(
                &mut engine,
                rows_of(
                    DEALS,
                    Some(json!({ "literal": {
                        "propertyDefinitionId": STAGE, "value": { "selectOption": WON }
                    } })),
                ),
            )
            .await,
            vec![format!("GraphqlSoupDatabaseRow:{DEAL_1}")]
        );

        // A realtime patch carries the new row; the table's rows now hold it.
        write(
            &mut engine,
            &page(vec![row(
                DEAL_3,
                DEALS,
                "2026-01-03T00:00:00Z",
                Some(("e0000000-0000-0000-0000-000000000003", WON)),
            )]),
        )
        .await;
        assert_eq!(
            local_rows(&mut engine, rows_of(DEALS, None)).await,
            vec![
                format!("GraphqlSoupDatabaseRow:{DEAL_3}"),
                format!("GraphqlSoupDatabaseRow:{DEAL_2}"),
                format!("GraphqlSoupDatabaseRow:{DEAL_1}"),
            ]
        );
    });
}
