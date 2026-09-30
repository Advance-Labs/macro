use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use macro_user_id::user_id::MacroUserIdStr;
use serde_json::json;

use super::{MAX_ENTRIES_PER_USER, MAX_VALUE_BYTES, UserKvServiceImpl};
use crate::domain::models::{KvKey, KvNamespace, KvValue, UserKvEntry, UserKvError};
use crate::domain::ports::{UserKvRepo, UserKvService};

const USER_A: &str = "macro|user-a@macro.com";
const USER_B: &str = "macro|user-b@macro.com";

fn user(id: &str) -> MacroUserIdStr<'_> {
    MacroUserIdStr::parse_from_str(id).expect("valid user id")
}

fn ns(value: &str) -> KvNamespace {
    KvNamespace::parse(value).expect("valid namespace")
}

fn key(value: &str) -> KvKey {
    KvKey::parse(value).expect("valid key")
}

fn object(value: serde_json::Value) -> KvValue {
    value.as_object().expect("object").clone()
}

type Store = BTreeMap<(String, String, String), UserKvEntry>;

#[derive(Clone, Default)]
struct FakeUserKvRepo {
    entries: Arc<Mutex<Store>>,
    fail: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("fake user kv repository error")]
struct FakeRepoError;

impl FakeUserKvRepo {
    fn failing() -> Self {
        Self {
            fail: true,
            ..Self::default()
        }
    }

    fn seed(&self, user_id: &str, count: usize) {
        let mut entries = self.entries.lock().unwrap();
        for i in 0..count {
            let entry = UserKvEntry {
                namespace: ns("seed"),
                key: key(&format!("k{i}")),
                value: KvValue::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            entries.insert((user_id.into(), "seed".into(), format!("k{i}")), entry);
        }
    }

    fn check(&self) -> Result<(), FakeRepoError> {
        if self.fail {
            Err(FakeRepoError)
        } else {
            Ok(())
        }
    }
}

fn id(
    user_id: &MacroUserIdStr<'_>,
    namespace: &KvNamespace,
    key: &KvKey,
) -> (String, String, String) {
    (
        user_id.as_ref().to_string(),
        namespace.as_str().to_string(),
        key.as_str().to_string(),
    )
}

impl UserKvRepo for FakeUserKvRepo {
    type Err = FakeRepoError;

    async fn list_entries(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
    ) -> Result<Vec<UserKvEntry>, Self::Err> {
        self.check()?;
        Ok(self
            .entries
            .lock()
            .unwrap()
            .iter()
            .filter(|((u, n, _), _)| u == user_id.as_ref() && n == namespace.as_str())
            .map(|(_, entry)| entry.clone())
            .collect())
    }

    async fn get_entry(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
        key: &KvKey,
    ) -> Result<Option<UserKvEntry>, Self::Err> {
        self.check()?;
        Ok(self
            .entries
            .lock()
            .unwrap()
            .get(&id(user_id, namespace, key))
            .cloned())
    }

    async fn upsert_entry(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
        key: &KvKey,
        value: &KvValue,
    ) -> Result<UserKvEntry, Self::Err> {
        self.check()?;
        let mut entries = self.entries.lock().unwrap();
        let now = Utc::now();
        let created_at = entries
            .get(&id(user_id, namespace, key))
            .map_or(now, |entry| entry.created_at);
        let entry = UserKvEntry {
            namespace: namespace.clone(),
            key: key.clone(),
            value: value.clone(),
            created_at,
            updated_at: now,
        };
        entries.insert(id(user_id, namespace, key), entry.clone());
        Ok(entry)
    }

    async fn delete_entry(
        &self,
        user_id: &MacroUserIdStr<'_>,
        namespace: &KvNamespace,
        key: &KvKey,
    ) -> Result<bool, Self::Err> {
        self.check()?;
        Ok(self
            .entries
            .lock()
            .unwrap()
            .remove(&id(user_id, namespace, key))
            .is_some())
    }

    async fn count_entries(&self, user_id: &MacroUserIdStr<'_>) -> Result<i64, Self::Err> {
        self.check()?;
        Ok(self
            .entries
            .lock()
            .unwrap()
            .keys()
            .filter(|(u, _, _)| u == user_id.as_ref())
            .count() as i64)
    }
}

#[tokio::test]
async fn put_then_get_and_list_return_the_value_scoped_to_the_user() {
    let service = UserKvServiceImpl::new(FakeUserKvRepo::default());
    let value = object(json!({ "status": "active", "step": 2 }));

    let stored = service
        .put_entry(&user(USER_A), &ns("tours"), &key("calendar"), value.clone())
        .await
        .expect("put should succeed");
    assert_eq!(stored.value, value);

    let fetched = service
        .get_entry(&user(USER_A), &ns("tours"), &key("calendar"))
        .await
        .expect("get should succeed");
    assert_eq!(fetched.value, value);

    let listed = service
        .list_entries(&user(USER_A), &ns("tours"))
        .await
        .expect("list should succeed");
    assert_eq!(listed.len(), 1);

    assert!(matches!(
        service
            .get_entry(&user(USER_B), &ns("tours"), &key("calendar"))
            .await,
        Err(UserKvError::NotFound)
    ));
    assert!(
        service
            .list_entries(&user(USER_B), &ns("tours"))
            .await
            .expect("list should succeed")
            .is_empty()
    );
}

#[tokio::test]
async fn put_replaces_the_whole_value() {
    let service = UserKvServiceImpl::new(FakeUserKvRepo::default());
    let target = (&user(USER_A), &ns("tours"), &key("calendar"));
    service
        .put_entry(
            target.0,
            target.1,
            target.2,
            object(json!({ "status": "active", "step": 2 })),
        )
        .await
        .expect("first put");
    let replaced = service
        .put_entry(
            target.0,
            target.1,
            target.2,
            object(json!({ "status": "completed" })),
        )
        .await
        .expect("second put");
    assert_eq!(replaced.value, object(json!({ "status": "completed" })));
}

#[tokio::test]
async fn put_rejects_values_over_the_size_limit() {
    let service = UserKvServiceImpl::new(FakeUserKvRepo::default());
    let too_big = object(json!({ "blob": "x".repeat(MAX_VALUE_BYTES) }));
    let result = service
        .put_entry(&user(USER_A), &ns("tours"), &key("calendar"), too_big)
        .await;
    assert!(matches!(
        result,
        Err(UserKvError::ValueTooLarge { limit, .. }) if limit == MAX_VALUE_BYTES
    ));

    // Just under the limit is fine: `{"blob":"…"}` adds 11 bytes of JSON.
    let fits = object(json!({ "blob": "x".repeat(MAX_VALUE_BYTES - 11) }));
    assert!(
        service
            .put_entry(&user(USER_A), &ns("tours"), &key("calendar"), fits)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn new_keys_stop_at_the_entry_limit_but_existing_keys_can_be_replaced() {
    let repo = FakeUserKvRepo::default();
    repo.seed(USER_A, MAX_ENTRIES_PER_USER);
    let service = UserKvServiceImpl::new(repo);

    let new_key = service
        .put_entry(
            &user(USER_A),
            &ns("tours"),
            &key("calendar"),
            KvValue::new(),
        )
        .await;
    assert!(matches!(
        new_key,
        Err(UserKvError::EntryLimitReached { limit }) if limit == MAX_ENTRIES_PER_USER
    ));

    let existing = service
        .put_entry(
            &user(USER_A),
            &ns("seed"),
            &key("k0"),
            object(json!({ "a": 1 })),
        )
        .await;
    assert!(existing.is_ok(), "replacing an existing key is allowed");

    let other_user = service
        .put_entry(
            &user(USER_B),
            &ns("tours"),
            &key("calendar"),
            KvValue::new(),
        )
        .await;
    assert!(other_user.is_ok(), "the limit is per user");
}

#[tokio::test]
async fn delete_removes_the_entry_and_reports_missing_ones() {
    let service = UserKvServiceImpl::new(FakeUserKvRepo::default());
    service
        .put_entry(
            &user(USER_A),
            &ns("tours"),
            &key("calendar"),
            KvValue::new(),
        )
        .await
        .expect("put");
    service
        .delete_entry(&user(USER_A), &ns("tours"), &key("calendar"))
        .await
        .expect("delete should succeed");
    assert!(matches!(
        service
            .delete_entry(&user(USER_A), &ns("tours"), &key("calendar"))
            .await,
        Err(UserKvError::NotFound)
    ));
}

#[tokio::test]
async fn repository_failures_become_internal_errors() {
    let service = UserKvServiceImpl::new(FakeUserKvRepo::failing());
    assert!(matches!(
        service.list_entries(&user(USER_A), &ns("tours")).await,
        Err(UserKvError::Internal(_))
    ));
    assert!(matches!(
        service
            .put_entry(
                &user(USER_A),
                &ns("tours"),
                &key("calendar"),
                KvValue::new()
            )
            .await,
        Err(UserKvError::Internal(_))
    ));
}
