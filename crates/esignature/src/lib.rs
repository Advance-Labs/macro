#![deny(missing_docs)]
//! Native PDF envelopes, recipient capabilities, signing, and completion evidence.
//! Domain policy is independent of transport, database, PDF, and email adapters.

/// Envelope models, use cases, and capability ports.
pub mod domain;
/// Authenticated management and capability-based signing endpoints.
#[cfg(feature = "inbound")]
pub mod inbound;
/// Database, PDF, and email implementations.
pub mod outbound;
