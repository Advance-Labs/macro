//! Ports (trait contracts) for the per-user key-value domain.

use macro_user_id::user_id::MacroUserIdStr;

use crate::domain::models::{KvKey, KvNamespace, KvValue, UserKvEntry, UserKvError};

/// Outbound persistence port for key-value entries.
///
/// Every method is scoped to one user: another user's entry simply misses.
pub trait UserKvRepo: Send + Sync + 'static {
    /// The error type returned by repository operations.
    type Err: std::error::Error + Send + Sync + 'static;

    /// The user's entries in a namespace, ordered by key.
    fn list_entries(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
    ) -> impl Future<Output = Result<Vec<UserKvEntry>, Self::Err>> + Send;

    /// One entry, if it exists.
    fn get_entry(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
        key: &KvKey,
    ) -> impl Future<Output = Result<Option<UserKvEntry>, Self::Err>> + Send;

    /// Create the entry or replace its value, returning it as stored.
    fn upsert_entry(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
        key: &KvKey,
        value: &KvValue,
    ) -> impl Future<Output = Result<UserKvEntry, Self::Err>> + Send;

    /// Remove one entry. Returns `true` when a row was removed.
    fn delete_entry(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
        key: &KvKey,
    ) -> impl Future<Output = Result<bool, Self::Err>> + Send;

    /// How many entries the user has across all namespaces.
    fn count_entries(
        &self,
        user_id: &MacroUserIdStr<'_>,
    ) -> impl Future<Output = Result<i64, Self::Err>> + Send;
}

/// Inbound service port: the key-value API used by drivers (HTTP, and other
/// crates in-process).
pub trait UserKvService: Send + Sync + 'static {
    /// The user's entries in a namespace, ordered by key. Empty when there
    /// are none.
    fn list_entries(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
    ) -> impl Future<Output = Result<Vec<UserKvEntry>, UserKvError>> + Send;

    /// One entry. [UserKvError::NotFound] when it doesn't exist.
    fn get_entry(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
        key: &KvKey,
    ) -> impl Future<Output = Result<UserKvEntry, UserKvError>> + Send;

    /// Create the entry or replace its whole value.
    fn put_entry(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
        key: &KvKey,
        value: KvValue,
    ) -> impl Future<Output = Result<UserKvEntry, UserKvError>> + Send;

    /// Remove one entry. [UserKvError::NotFound] when it doesn't exist.
    fn delete_entry(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
        key: &KvKey,
    ) -> impl Future<Output = Result<(), UserKvError>> + Send;
}
