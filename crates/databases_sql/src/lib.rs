//! SQL over Macro databases on the server, for agents.
//!
//! The databases domain is typed: it lists databases, describes them and
//! applies ops. This adapter runs the `database_sql` engine over it the way
//! the browser does:
//!
//! - **Catalog:** the databases the viewer can reach, from the databases
//!   service's listing (which `entity_access` scopes), mapped onto the
//!   engine's shared schema.
//! - **Reads:** Soup, with the same filters the browser's GraphQL source
//!   sends, so both see the same rows; `macro.people` is the viewer's
//!   contacts.
//! - **Writes:** the statement's ops, applied by the databases service under
//!   an edit receipt for their database.
//!
//! It is composition-level: hosts build it from the domain services they
//! already run.
#![deny(missing_docs)]

mod catalog;
mod ops_sink;
mod outcome;
mod row_source;
mod service;
#[cfg(test)]
mod test_support;
#[cfg(feature = "ai_tools")]
pub mod toolset;

pub use outcome::{AlteredColumn, ResultColumn, ResultSet, SqlOutcome};
pub use service::{DatabasesSql, SqlError, SqlRequest};
