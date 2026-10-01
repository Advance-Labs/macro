use cache_core::{
    engine::{BeginOptimisticWrite, Engine, ReadResult},
    meta::Schema,
    record_selection::cache::RecordSelectionCache,
    store::{InMemoryStorage, Storage},
    value::EntityKey,
};
use pollster::block_on;
use serde_json::{Value, json};

fn descriptor() -> Value {
    serde_json::from_str(&Schema::compiled().to_json()).unwrap()
}
fn field(name: &str, named: &str, kind: &str, list: bool) -> Value {
    json!({ "name":name, "ty": { "name":named, "kind":kind, "nullable":false, "list":list, "item_nullable":false } })
}
fn extend_type(schema: &mut Value, name: &str, field: Value) {
    schema["types"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|ty| ty["name"] == name)
        .unwrap()["fields"]
        .as_array_mut()
        .unwrap()
        .push(field);
}
fn evolved() -> Schema {
    let mut schema = descriptor();
    extend_type(
        &mut schema,
        Schema::compiled().query_root(),
        field("otaMessage", "OtaMessage", "Composite", false),
    );
    extend_type(
        &mut schema,
        Schema::compiled().mutation_root().unwrap(),
        field("otaUpdate", "OtaMessage", "Composite", false),
    );
    schema["types"].as_array_mut().unwrap().push(json!({
        "name":"OtaMessage", "kind":"Object", "key_fields":["id"], "possible_types":[],
        "fields":[field("id", "ID", "Leaf", false), field("body", "String", "Leaf", false), field("calendarInvitations", "JSON", "OpaqueScalar", false)]
    }));
    Schema::from_json(&schema.to_string()).unwrap()
}
const OLD_QUERY: &str = "query { otaMessage { id body } }";
const QUERY: &str = "query { otaMessage { id body calendarInvitations } }";
const MUTATION: &str = "mutation { otaUpdate { id body calendarInvitations } }";
fn message(body: &str) -> Value {
    json!({"id":"message", "body":body,"calendarInvitations":[{"uid":"invite","sequence":1}]})
}
async fn hit(engine: &mut Engine<InMemoryStorage>) -> Value {
    match engine
        .read_query(None, QUERY, None, &Default::default())
        .await
        .unwrap()
    {
        ReadResult::Hit { data } => data,
        ReadResult::Miss => panic!("expected complete cached message"),
    }
}

#[test]
fn ota_fields_fill_on_online_fetch_and_survive_narrow_writes_and_reopen() {
    block_on(async {
        let mut engine = Engine::with_capacity(InMemoryStorage::new(), 1);
        let generation = engine.current_storage_generation().await.unwrap();
        assert!(
            engine
                .write_query(
                    None,
                    QUERY,
                    None,
                    &Default::default(),
                    &json!({"otaMessage":message("body")}),
                    None
                )
                .await
                .is_err()
        );
        let ack = engine.configure_schema(evolved()).await.unwrap();
        assert_eq!(ack.protocol_version, 1);
        engine
            .write_query(
                None,
                OLD_QUERY,
                None,
                &Default::default(),
                &json!({"otaMessage":{"id":"message","body":"cached"}}),
                None,
            )
            .await
            .unwrap();
        assert!(matches!(
            engine
                .read_query(None, QUERY, None, &Default::default())
                .await
                .unwrap(),
            ReadResult::Miss
        ));
        engine
            .write_query(
                None,
                QUERY,
                None,
                &Default::default(),
                &json!({"otaMessage":message("body")}),
                None,
            )
            .await
            .unwrap();
        engine
            .write_query(
                None,
                OLD_QUERY,
                None,
                &Default::default(),
                &json!({"otaMessage":{"id":"message","body":"updated"}}),
                None,
            )
            .await
            .unwrap();
        assert_eq!(hit(&mut engine).await["otaMessage"], message("updated"));
        assert_eq!(
            engine.current_storage_generation().await.unwrap(),
            generation
        );
        let mut reopened = Engine::with_capacity(engine.into_storage(), 1);
        // Older frontend reconnects: its subset must not discard newer metadata.
        reopened
            .configure_schema(Schema::compiled().clone())
            .await
            .unwrap();
        assert_eq!(reopened.schema().fingerprint(), ack.fingerprint);
        assert_eq!(hit(&mut reopened).await["otaMessage"], message("updated"));
        assert_eq!(
            reopened.current_storage_generation().await.unwrap(),
            generation
        );
        let mut independent = Engine::new(InMemoryStorage::new());
        assert!(independent.schema().type_meta("OtaMessage").is_none());
        assert!(
            independent
                .write_query(
                    None,
                    QUERY,
                    None,
                    &Default::default(),
                    &json!({"otaMessage":message("body")}),
                    None
                )
                .await
                .is_err()
        );
    });
}

#[test]
fn queued_mutation_replays_after_reopen_and_schema_rejection_keeps_it() {
    block_on(async {
        let mut engine = Engine::new(InMemoryStorage::new());
        engine.configure_schema(evolved()).await.unwrap();
        engine
            .write_query(
                None,
                QUERY,
                None,
                &Default::default(),
                &json!({"otaMessage":message("original")}),
                None,
            )
            .await
            .unwrap();
        engine
            .begin_optimistic_write(
                None,
                BeginOptimisticWrite {
                    uuid: "00000000-0000-4000-8000-000000000001",
                    query: MUTATION,
                    operation_name: None,
                    variables: &Default::default(),
                    data: &json!({"otaUpdate":message("optimistic")}),
                    link_patches: &[],
                    revalidations: &[],
                    created_at_ms: 1,
                },
            )
            .await
            .unwrap();
        let mut incompatible: Value = serde_json::from_str(&evolved().to_json()).unwrap();
        let message_type = incompatible["types"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|ty| ty["name"] == "OtaMessage")
            .unwrap();
        message_type["fields"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|field| field["name"] == "body")
            .unwrap()["ty"]["name"] = json!("Int");
        let before = engine.schema().fingerprint().to_owned();
        assert!(
            engine
                .configure_schema(Schema::from_json(&incompatible.to_string()).unwrap())
                .await
                .is_err()
        );
        assert_eq!(engine.schema().fingerprint(), before);
        assert_eq!(
            engine.storage().load_mutation_queue().await.unwrap().len(),
            1
        );
        let mut reopened = Engine::new(engine.into_storage());
        // Lazy core replay also restores persisted metadata before parsing the queue.
        assert_eq!(
            hit(&mut reopened).await["otaMessage"],
            message("optimistic")
        );
        assert_eq!(
            reopened
                .storage()
                .load_mutation_queue()
                .await
                .unwrap()
                .len(),
            1
        );
        let mut selections = RecordSelectionCache::default();
        let fragment = "fragment Message on OtaMessage { id body calendarInvitations }";
        assert!(selections.get(fragment.into(), "Message".into()).is_err());
        let selection = selections
            .get_with_schema(reopened.schema(), fragment.into(), "Message".into())
            .unwrap();
        let records = reopened
            .read_records_by_keys(&selection, &[EntityKey("OtaMessage:message".into())])
            .await
            .unwrap();
        assert_eq!(records.value[0].record, message("optimistic"));
    });
}

#[test]
fn rejects_unsupported_protocol_bad_references_and_key_changes() {
    let baseline = Schema::compiled();
    let mut invalid = descriptor();
    invalid["protocolVersion"] = json!(2);
    assert!(Schema::from_json(&invalid.to_string()).is_err());
    let mut invalid = descriptor();
    extend_type(
        &mut invalid,
        baseline.query_root(),
        field("missing", "Absent", "Composite", false),
    );
    assert!(Schema::from_json(&invalid.to_string()).is_err());
    let mut invalid = descriptor();
    let ty = invalid["types"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|ty| ty["name"] == "GraphqlSoupEmailMessage")
        .unwrap();
    ty["key_fields"] = Value::Null;
    ty["fields"]
        .as_array_mut()
        .unwrap()
        .retain(|field| field["name"] != "id");
    assert!(
        baseline
            .merge(&Schema::from_json(&invalid.to_string()).unwrap())
            .is_err()
    );
    let mut invalid = descriptor();
    let duplicate = invalid["types"][0].clone();
    invalid["types"].as_array_mut().unwrap().push(duplicate);
    assert!(Schema::from_json(&invalid.to_string()).is_err());
}

#[test]
fn frontend_generated_metadata_matches_the_rust_schema_exactly() {
    let frontend = Schema::from_json(include_str!(
        "../../../../apps/web/src/lib/graphql-cache/generated/runtime-schema.json"
    ))
    .unwrap();
    assert_eq!(frontend, *Schema::compiled());
}
