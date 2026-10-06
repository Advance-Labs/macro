//! Inbound adapters for the CRM domain.

#[cfg(feature = "axum")]
pub mod axum_extractors;
#[cfg(feature = "axum")]
pub mod axum_router;
#[cfg(feature = "call_link")]
pub mod call_archived;
/// Names the other end of a phone call from the call owner's team CRM.
#[cfg(feature = "call_link")]
pub mod phone_contacts;
#[cfg(feature = "ai_tools")]
pub mod toolset;
