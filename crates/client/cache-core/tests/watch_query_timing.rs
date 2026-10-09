//! Prints the costs that decide whether document queries should keep sending
//! response-path patches. Ignored by default; run in release mode:
//! `cargo test --release -p cache-core --test watch_query_timing -- --ignored --nocapture`

use cache_core::engine::watch_query::QueryUpdate;
use cache_core::engine::{Engine, ReadResult};
use cache_core::revision::CacheRevision;
use cache_core::store::InMemoryStorage;
use cache_core::value::{CacheValue, EntityKey, Record};
use pollster::block_on;
use serde_json::{Map, Value as Json, json};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const PAGE: &str = r#"
query Page($input: SoupInput!) {
  user { id soup(input: $input) { items {
    __typename id frecencyScore
    ... on GraphqlSoupDocument {
      documentName: name ownerId fileType projectId createdAt updatedAt viewedAt deletedAt
      subType { __typename ... on GraphqlTaskSubType { isCompleted } }
      properties { id propertyDefinitionId displayName dataType isMultiSelect value {
        __typename
        ... on GraphqlSelectOptionPropertyValue { optionIds }
        ... on GraphqlStringPropertyValue { value }
      } }
    }
  } nextCursor } }
}"#;
const SAMPLES: usize = 200;

fn variables(rows: usize) -> Map<String, Json> {
    json!({"input": {"initial": {"limit": rows}}})
        .as_object()
        .unwrap()
        .clone()
}

fn page(rows: usize) -> Json {
    let items: Vec<_> = (0..rows)
        .map(|i| {
            json!({
                "__typename": "GraphqlSoupDocument",
                "id": format!("doc-{i}"),
                "frecencyScore": 0.5,
                "documentName": format!("Document {i}"),
                "ownerId": "macro|owner@example.com",
                "fileType": "md",
                "projectId": if i % 3 == 0 { Json::Null } else { json!("project-1") },
                "createdAt": "2026-10-01T00:00:00Z",
                "updatedAt": "2026-10-02T00:00:00Z",
                "viewedAt": "2026-10-03T00:00:00Z",
                "deletedAt": null,
                "subType": {"__typename": "GraphqlTaskSubType", "isCompleted": false},
                "properties": [
                    {
                        "id": format!("status-{i}"), "propertyDefinitionId": "status",
                        "displayName": "Status", "dataType": "SELECT_STRING", "isMultiSelect": false,
                        "value": {"__typename": "GraphqlSelectOptionPropertyValue", "optionIds": ["todo"]}
                    },
                    {
                        "id": format!("note-{i}"), "propertyDefinitionId": "note",
                        "displayName": "Note", "dataType": "STRING", "isMultiSelect": false,
                        "value": {"__typename": "GraphqlStringPropertyValue", "value": "note"}
                    }
                ]
            })
        })
        .collect();
    json!({"user": {"id": "viewer", "soup": {"items": items, "nextCursor": null}}})
}

async fn put(
    engine: &mut Engine<InMemoryStorage>,
    key: EntityKey<'static>,
    field: &str,
    value: CacheValue,
) {
    engine
        .put_records_with_projections(
            None,
            vec![(
                key,
                Record {
                    fields: BTreeMap::from([(field.into(), value)]),
                },
            )],
            vec![],
        )
        .await
        .unwrap();
}

fn revision(update: &QueryUpdate) -> CacheRevision {
    match update {
        QueryUpdate::Hit { revision, .. }
        | QueryUpdate::Patch { revision, .. }
        | QueryUpdate::Miss { revision } => revision.parse().unwrap(),
    }
}

fn median(mut samples: Vec<Duration>) -> f64 {
    samples.sort();
    samples[samples.len() / 2].as_secs_f64() * 1e6
}

/// Times one watched read after each untimed edit and checks its patch count.
async fn time_watch(
    engine: &mut Engine<InMemoryStorage>,
    cursor: &mut CacheRevision,
    rows: usize,
    expected_patches: usize,
    mut edit: impl FnMut(usize) -> (EntityKey<'static>, &'static str, CacheValue),
) -> f64 {
    let mut samples = Vec::with_capacity(SAMPLES);
    for sample in 0..SAMPLES {
        let (key, field, value) = edit(sample);
        put(engine, key, field, value).await;
        let start = Instant::now();
        let update = engine
            .watch_query(1, PAGE, None, &variables(rows), &[], Some(*cursor))
            .await
            .unwrap();
        samples.push(start.elapsed());
        *cursor = revision(&update);
        let QueryUpdate::Patch { patches, .. } = update else {
            panic!("expected an incremental update: {update:?}");
        };
        assert_eq!(patches.len(), expected_patches);
    }
    median(samples)
}

#[test]
#[ignore = "prints timings; run in release mode"]
fn watch_query_costs() {
    println!(
        "| rows | full read_query | serialize Hit | watch: row field | watch: nested property | watch: unrelated record |"
    );
    println!("|---:|---:|---:|---:|---:|---:|");
    for rows in [100, 500] {
        block_on(async {
            let mut engine = Engine::new(InMemoryStorage::new());
            let variables = variables(rows);
            engine
                .write_query(None, PAGE, None, &variables, &page(rows), None)
                .await
                .unwrap();

            let mut reads = Vec::with_capacity(SAMPLES);
            let mut serialized = Vec::with_capacity(SAMPLES);
            for _ in 0..SAMPLES {
                let start = Instant::now();
                let result = engine
                    .read_query(None, PAGE, None, &variables)
                    .await
                    .unwrap();
                reads.push(start.elapsed());
                let ReadResult::Hit { data } = result else {
                    panic!("seeded page must hit");
                };
                let start = Instant::now();
                std::hint::black_box(serde_json::to_string(&data).unwrap());
                serialized.push(start.elapsed());
            }

            let first = engine
                .watch_query(1, PAGE, None, &variables, &[], None)
                .await
                .unwrap();
            assert!(matches!(first, QueryUpdate::Hit { .. }));
            let mut cursor = revision(&first);
            let field = time_watch(&mut engine, &mut cursor, rows, 1, |sample| {
                (
                    EntityKey::entity(
                        "GraphqlSoupDocument",
                        &[&format!("doc-{}", sample * 7 % rows)],
                    ),
                    "name",
                    CacheValue::String(format!("Renamed {sample}")),
                )
            })
            .await;
            let nested = time_watch(&mut engine, &mut cursor, rows, 1, |sample| {
                (
                    EntityKey::entity(
                        "GraphqlProperty",
                        &[&format!("status-{}", sample * 7 % rows)],
                    ),
                    "value",
                    CacheValue::Object(BTreeMap::from([
                        (
                            "__typename".into(),
                            CacheValue::String("GraphqlSelectOptionPropertyValue".into()),
                        ),
                        (
                            "optionIds".into(),
                            CacheValue::List(vec![CacheValue::String(format!("option-{sample}"))]),
                        ),
                    ])),
                )
            })
            .await;
            let unrelated = time_watch(&mut engine, &mut cursor, rows, 0, |sample| {
                (
                    EntityKey::entity("GraphqlSoupDocument", &["unrelated"]),
                    "name",
                    CacheValue::String(format!("Unrelated {sample}")),
                )
            })
            .await;
            println!(
                "| {rows} | {:.1} µs | {:.1} µs | {field:.1} µs | {nested:.1} µs | {unrelated:.1} µs |",
                median(reads),
                median(serialized),
            );
        });
    }
}
