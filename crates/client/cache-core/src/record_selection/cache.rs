//! Bounded reuse of validated fragment plans across cache hosts.
use super::{RecordSelection, RecordSelectionError};
use lru::LruCache;
use std::{num::NonZeroUsize, sync::Arc};

const CAPACITY: usize = 128;

/// Reuses successful fragment plans by document text and fragment name.
pub struct RecordSelectionCache {
    plans: LruCache<(String, String, String), Arc<RecordSelection>>,
}

impl Default for RecordSelectionCache {
    fn default() -> Self {
        Self {
            plans: LruCache::new(NonZeroUsize::new(CAPACITY).unwrap()),
        }
    }
}

impl RecordSelectionCache {
    pub fn get(
        &mut self,
        document: String,
        fragment: String,
    ) -> Result<Arc<RecordSelection>, RecordSelectionError> {
        self.get_with_schema(crate::meta::Schema::compiled(), document, fragment)
    }

    /// Returns a validated plan, parsing only on a cache miss.
    pub fn get_with_schema(
        &mut self,
        schema: &crate::meta::Schema,
        document: String,
        fragment: String,
    ) -> Result<Arc<RecordSelection>, RecordSelectionError> {
        let key = (document, fragment, schema.fingerprint().to_owned());
        if let Some(selection) = self.plans.get(&key) {
            return Ok(Arc::clone(selection));
        }
        let selection = Arc::new(RecordSelection::parse_with_schema(schema, &key.0, &key.1)?);
        self.plans.put(key, Arc::clone(&selection));
        Ok(selection)
    }
}

#[cfg(test)]
mod test;
