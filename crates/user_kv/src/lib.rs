#![deny(missing_docs)]
//! A generic per-user key-value store, following the hexagonal architecture
//! pattern.
//!
//! Each entry is a small JSON object owned by one user and addressed by a
//! namespace and a key, e.g. namespace `tours`, key `calendar`. It holds app
//! state that doesn't warrant its own typed table. Namespaces are free-form
//! slugs; the service enforces value size and a per-user entry limit. Writes
//! replace the whole value.
//!
//! # Architecture
//!
//! - **domain**: models, ports, and the service implementation.
//! - **inbound**: driving adapters (Axum HTTP router).
//! - **outbound**: driven adapters (Postgres repository).

pub mod domain;

#[cfg(feature = "inbound")]
pub mod inbound;

#[cfg(feature = "outbound")]
pub mod outbound;
