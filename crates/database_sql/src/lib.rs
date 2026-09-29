//! The SQL subset agents and the console use to read and write Macro
//! databases.
//!
//! The grammar is the contract: only statements this crate can carry all the
//! way to a GraphQL query and a row fold are parseable, so nothing is accepted
//! here and rejected later. The pipeline is
//!
//! ```text
//! sql string ── parse ──▶ subset AST ── resolve ──▶ (catalog-bound query)
//!            ── split ──▶ GraphQL query + post-processing ── fold ──▶ rows
//! ```
//!
//! This crate compiles natively and to `wasm32`; keep it free of native-only
//! dependencies.
#![deny(missing_docs)]

pub mod catalog;
pub mod fold;
pub mod parse;
pub mod resolve;
pub mod run;
pub mod split;
#[cfg(test)]
mod test_support;

pub use catalog::Catalog;
pub use fold::{Bin, Cell, Row, Table, fold_bins, fold_rows};
pub use parse::{ParseError, parse};
pub use resolve::{CompileError, Query, ResolveError, compile, resolve};
pub use run::{
    Outcome, OutcomeColumn, OutcomeKind, Page, RowFailure, RowSource, RowWriter, RunError,
    SourceError, WriteError, run,
};
pub use split::{GqlQuery, Plan, Shape, split};
