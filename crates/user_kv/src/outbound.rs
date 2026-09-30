//! Outbound (driven) adapters for the per-user key-value store.

#[cfg(feature = "postgres")]
pub mod pg_user_kv_repo;
