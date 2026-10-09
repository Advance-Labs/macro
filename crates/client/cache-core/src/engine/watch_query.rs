//! Incremental reads of ordinary GraphQL queries. A watch retains the response
//! it last published. Later reads skip work when no dependency changed, and
//! otherwise re-read the whole query and diff it, so no selected field can be
//! missed. Transports only apply response paths.

use super::*;
use crate::engine::live_query::LiveFieldPatch;
use serde::Serialize;

mod diff;

const WATCH_CAPACITY: usize = 64;
const WATCH_BYTES: usize = 16 * 1024 * 1024;
// Rough in-memory cost of one retained dependency key, beyond its text.
const RECORD_KEY_BYTES: usize = 64;

/// An atomic query read. A patch is applicable only to the exact revision the
/// subscriber supplied; eviction, spec changes and revision gaps reset it.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum QueryUpdate {
    /// Complete replacement, also establishing a new patch base.
    Hit { data: Json, revision: String },
    /// Selected field replacements, including an empty update for no change.
    Patch {
        patches: Vec<LiveFieldPatch>,
        revision: String,
    },
    /// Required data is missing; the caller must use its normal network policy.
    Miss { revision: String },
}

#[derive(PartialEq, Eq)]
struct QuerySpec {
    query: String,
    operation_name: Option<String>,
    variables: serde_json::Map<String, Json>,
    entity_resolvers: Vec<EntityResolver>,
}

impl QuerySpec {
    fn retained_bytes(&self) -> usize {
        self.query.len() + serde_json::to_vec(&self.variables).map_or(0, |json| json.len())
    }
}

struct QueryWatch {
    spec: QuerySpec,
    /// The subscriber's current result at `revision`.
    data: Json,
    data_bytes: usize,
    /// Every record the read traversed, including absent ones.
    records: BTreeSet<EntityKey<'static>>,
    revision: CacheRevision,
    bytes: usize,
}

impl QueryWatch {
    fn new(
        spec: QuerySpec,
        data: Json,
        data_bytes: usize,
        records: BTreeSet<EntityKey<'static>>,
        revision: CacheRevision,
    ) -> Self {
        let bytes = spec.retained_bytes()
            + data_bytes
            + records
                .iter()
                .map(|key| key.as_ref().len() + RECORD_KEY_BYTES)
                .sum::<usize>();
        Self {
            spec,
            data,
            data_bytes,
            records,
            revision,
            bytes,
        }
    }
}

pub(super) struct QueryWatches {
    views: LruCache<OpId, QueryWatch>,
    bytes: usize,
}

impl Default for QueryWatches {
    fn default() -> Self {
        Self {
            views: LruCache::new(NonZeroUsize::new(WATCH_CAPACITY).unwrap()),
            bytes: 0,
        }
    }
}

impl QueryWatches {
    fn pop(&mut self, op: OpId) -> Option<QueryWatch> {
        let view = self.views.pop(&op)?;
        self.bytes -= view.bytes;
        Some(view)
    }

    pub(super) fn remove(&mut self, op: OpId) {
        self.pop(op);
    }

    fn put(&mut self, op: OpId, view: QueryWatch) {
        if view.bytes > WATCH_BYTES {
            return;
        }
        while self.bytes + view.bytes > WATCH_BYTES || self.views.len() == WATCH_CAPACITY {
            let Some((_, old)) = self.views.pop_lru() else {
                break;
            };
            self.bytes -= old.bytes;
        }
        self.bytes += view.bytes;
        self.views.put(op, view);
    }
}

impl<S: Storage> Engine<S> {
    /// Watch any cache-readable query using its ordinary document and variables.
    /// The caller owns `op_id` until `teardown_operation`. Retention is bounded;
    /// losing a watch only causes a replacement read, never a missed update.
    pub async fn watch_query(
        &mut self,
        op_id: OpId,
        query: &str,
        operation_name: Option<&str>,
        variables: &serde_json::Map<String, Json>,
        entity_resolvers: &[EntityResolver],
        since: Option<CacheRevision>,
    ) -> Result<QueryUpdate, EngineError<S::Error>> {
        self.hydrate_optimistic().await?;
        let spec = QuerySpec {
            query: query.to_owned(),
            operation_name: operation_name.map(str::to_owned),
            variables: variables.clone(),
            entity_resolvers: entity_resolvers.to_vec(),
        };
        // A journal barrier cannot say what changed, so it publishes a replacement.
        let mut base = None;
        if let Some(mut view) = self.query_watches.pop(op_id)
            && view.spec == spec
            && Some(view.revision) == since
            && let Some(changes) = self
                .live_queries
                .changes_since(view.revision, self.revision)
        {
            if changes.records.is_disjoint(&view.records) {
                view.revision = self.revision;
                self.query_watches.put(op_id, view);
                return Ok(QueryUpdate::Patch {
                    patches: Vec::new(),
                    revision: self.revision.to_string(),
                });
            }
            base = Some(view);
        }
        let result = self
            .read_query_with_entity_resolvers(
                Some(op_id),
                query,
                operation_name,
                variables,
                entity_resolvers,
            )
            .await?;
        let revision = self.revision.to_string();
        let ReadResult::Hit { data } = result else {
            return Ok(QueryUpdate::Miss { revision });
        };
        // The read just registered exactly the records it traversed. Without
        // them a retained watch could skip a relevant change, so keep none.
        let records = match (base.as_mut(), self.deps.op_records(op_id)) {
            (_, None) => return Ok(QueryUpdate::Hit { data, revision }),
            (Some(view), Some(records)) if view.records == *records => {
                std::mem::take(&mut view.records)
            }
            (_, Some(records)) => records.clone(),
        };
        if let Some(view) = base
            && let Some(diff) = diff::diff_response(&view.data, &data)
        {
            let data_bytes = view.data_bytes.saturating_add_signed(diff.byte_delta);
            self.query_watches.put(
                op_id,
                QueryWatch::new(spec, data, data_bytes, records, self.revision),
            );
            return Ok(QueryUpdate::Patch {
                patches: diff.patches,
                revision,
            });
        }
        let data_bytes = diff::json_bytes(&data);
        self.query_watches.put(
            op_id,
            QueryWatch::new(spec, data.clone(), data_bytes, records, self.revision),
        );
        Ok(QueryUpdate::Hit { data, revision })
    }
}

#[cfg(test)]
mod test;
