//! Domain layer for the per-user key-value store.

pub mod models;
#[cfg(feature = "ports")]
pub mod ports;
#[cfg(feature = "ports")]
pub mod service;
