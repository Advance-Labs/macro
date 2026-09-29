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
pub mod parse;
pub mod resolve;

pub use catalog::Catalog;
pub use parse::{ParseError, parse};
pub use resolve::{CompileError, Query, ResolveError, compile, resolve};
