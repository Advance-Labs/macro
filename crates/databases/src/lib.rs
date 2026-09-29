#![deny(missing_docs)]
//! Macro Databases: user-facing tables of typed rows, queried and mutated
//! through SQL, following the hexagonal architecture pattern.
//!
//! A database is a collection of tables (tabs). Columns are bindings to
//! `models_properties` property definitions, so the existing property type
//! system (data types, select options, entity references) carries directly
//! over. A row is a `DATABASE_ROW` entity whose cells are its entity
//! properties; the `database_rows` table holds only the row's identity and
//! its place in the table.
//!
//! # The SQL-first surface
//!
//! There are no cell/row CRUD endpoints. The public write and read verb is
//! a small SQL subset, compiled by the `database_sql` crate:
//!
//! 1. Build the viewer's catalog from their grants — an unreadable table does
//!    not exist to the statement.
//! 2. Compile the statement against it: names resolved, literals typed, the
//!    filter split into what Soup can evaluate and what is folded here.
//! 3. Read the rows through the row and cell stores and fold the answer, or
//!    write through them one row at a time.
//!
//! Schema operations (create database/table/column) remain small structured
//! endpoints because property definitions carry configuration DDL cannot
//! express.
//!
//! # Architecture
//!
//! - **domain**: models, ports, and the service implementation (all policy).
//! - **inbound**: the Axum router (SQL exec, schema ops) and the AI toolset.
//! - **outbound**: Postgres repositories, the cell store over the properties
//!   adapter, and the table-event publisher.

pub mod domain;

#[cfg(any(feature = "inbound", feature = "ai_tools"))]
pub mod inbound;

#[cfg(feature = "outbound")]
pub mod outbound;
