//! SQL over Macro databases for agents: the `database_sql` engine run over
//! Soup, contacts and the databases service's ops, as the viewer.
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
