//! User key-value service implementation.

#[cfg(test)]
mod test;

use macro_user_id::user_id::MacroUserIdStr;

use crate::domain::models::{KvKey, KvNamespace, KvValue, UserKvEntry, UserKvError};
use crate::domain::ports::{UserKvRepo, UserKvService};

/// Largest value accepted, measured as compact JSON.
pub const MAX_VALUE_BYTES: usize = 16 * 1024;

/// Most entries one user may hold across all namespaces.
pub const MAX_ENTRIES_PER_USER: usize = 1000;

/// Concrete key-value service backed by a [UserKvRepo].
#[derive(Debug, Clone)]
pub struct UserKvServiceImpl<R> {
    repo: R,
}

impl<R> UserKvServiceImpl<R>
where
    R: UserKvRepo,
{
    /// Create a key-value service backed by the provided repository.
    pub fn new(repo: R) -> Self {
        Self { repo }
    }
}

fn internal<E: std::error::Error + Send + Sync + 'static>(error: E) -> UserKvError {
    rootcause::Report::new(error).into_dynamic().into()
}

impl<R> UserKvService for UserKvServiceImpl<R>
where
    R: UserKvRepo,
{
    #[tracing::instrument(err, skip_all, fields(namespace = %namespace))]
    async fn list_entries(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
    ) -> Result<Vec<UserKvEntry>, UserKvError> {
        self.repo
            .list_entries(user_id, namespace)
            .await
            .map_err(internal)
    }

    #[tracing::instrument(err, skip_all, fields(namespace = %namespace, key = %key))]
    async fn get_entry(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
        key: &KvKey,
    ) -> Result<UserKvEntry, UserKvError> {
        self.repo
            .get_entry(user_id, namespace, key)
            .await
            .map_err(internal)?
            .ok_or(UserKvError::NotFound)
    }

    #[tracing::instrument(err, skip_all, fields(namespace = %namespace, key = %key))]
    async fn put_entry(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
        key: &KvKey,
        value: KvValue,
    ) -> Result<UserKvEntry, UserKvError> {
        let size = serde_json::to_vec(&value).map_err(internal)?.len();
        if size > MAX_VALUE_BYTES {
            return Err(UserKvError::ValueTooLarge {
                size,
                limit: MAX_VALUE_BYTES,
            });
        }

        // Replacing an existing entry never counts against the limit. The
        // check is advisory: concurrent inserts of new keys can pass it
        // together, which only overshoots by a handful of rows.
        let exists = self
            .repo
            .get_entry(user_id, namespace, key)
            .await
            .map_err(internal)?
            .is_some();
        if !exists {
            let count = self.repo.count_entries(user_id).await.map_err(internal)?;
            if count as usize >= MAX_ENTRIES_PER_USER {
                return Err(UserKvError::EntryLimitReached {
                    limit: MAX_ENTRIES_PER_USER,
                });
            }
        }

        self.repo
            .upsert_entry(user_id, namespace, key, &value)
            .await
            .map_err(internal)
    }

    #[tracing::instrument(err, skip_all, fields(namespace = %namespace, key = %key))]
    async fn delete_entry(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
        key: &KvKey,
    ) -> Result<(), UserKvError> {
        if self
            .repo
            .delete_entry(user_id, namespace, key)
            .await
            .map_err(internal)?
        {
            Ok(())
        } else {
            Err(UserKvError::NotFound)
        }
    }
}
