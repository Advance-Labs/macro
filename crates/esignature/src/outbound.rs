/// SES/SMTP invitation delivery.
#[cfg(feature = "mail")]
pub mod mail;
/// Immutable PDF inspection and completed document rendering.
#[cfg(feature = "pdf")]
pub mod pdf;
/// PostgreSQL aggregate storage with optimistic concurrency.
#[cfg(feature = "postgres")]
pub mod postgres;
