//! Recoverable mutation intent, stored independently of optimistic layers.
//! The catalog uses the existing records transaction and format, so adding it
//! never changes the database namespace or strands queued writes on upgrade.
use crate::{
    queue::decode_optimistic_source,
    value::{CacheValue, EntityKey, Record},
};
use serde_json::Value as Json;
use uuid::Uuid;

/// Reserved metadata on an optimistic response; never part of the wire request.
pub const FIELD: &str = "__durableIntent";
/// Reserved catalog record type.
pub const TYPENAME: &str = "CacheMutationIntents";
/// Catalog key shared by browser and native storage.
pub fn key() -> EntityKey<'static> {
    EntityKey::entity(TYPENAME, &["catalog"])
}

/// Read opt-in intent metadata from a persisted optimistic source.
pub fn source_metadata(source: &str) -> Option<Json> {
    decode_optimistic_source(source)
        .ok()?
        .mutation_data
        .get(FIELD)
        .filter(|v| v.is_object())
        .cloned()
}

/// Explicit cancellation replaces the old request even when its lease is live.
pub fn replaces(data: &Json) -> bool {
    data.get(FIELD)
        .and_then(|v| v.get("replace"))
        .and_then(Json::as_bool)
        == Some(true)
}

/// Merge one intent into the durable catalog at enqueue/settlement.
pub fn update(
    record: &mut Record,
    uuid: Uuid,
    metadata: &Json,
    phase: &str,
    response: Option<&Json>,
    locally_cancelled: bool,
) {
    let value = serde_json::json!({"uuid": uuid, "metadata": metadata, "phase": phase, "response": response, "locallyCancelled": locally_cancelled});
    record
        .fields
        .insert(uuid.to_string(), CacheValue::String(value.to_string()));
}

/// Decode the catalog without requiring a generated GraphQL selection.
pub fn values(record: Option<Record>) -> Vec<Json> {
    record
        .into_iter()
        .flat_map(|record| record.fields.into_values())
        .filter_map(|value| {
            if let CacheValue::String(value) = value {
                serde_json::from_str(&value).ok()
            } else {
                None
            }
        })
        .collect()
}

/// A cancellation of a cancellation must retain the original send's uncertainty.
pub fn locally_cancelled(previous: Option<&str>, source: &str, attempt_count: u32) -> bool {
    let already_cancelled = previous
        .and_then(|value| serde_json::from_str::<Json>(value).ok())
        .and_then(|value| value.get("locallyCancelled").and_then(Json::as_bool))
        == Some(true);
    let was_replacement = source_metadata(source)
        .as_ref()
        .and_then(|value| value.get("replace"))
        .and_then(Json::as_bool)
        == Some(true);
    already_cancelled || (!was_replacement && attempt_count == 0)
}
