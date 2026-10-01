#![deny(missing_docs)]
//! Macro Databases: user-facing tables of typed rows, following the
//! hexagonal architecture pattern.
//!
//! A database is a collection of tables (tabs). Columns are bindings to
//! `models_properties` property definitions, so the existing property type
//! system (data types, select options, entity references) carries directly
//! over. A row is a `DATABASE_ROW` entity whose cells are its entity
//! properties; the `database_rows` table holds only the row's identity and
//! its place in the table.
//!
//! # A typed surface
//!
//! Rows are read as Soup items. Every data write is a batch of typed ops
//! (`models_databases::DatabaseOp`), applied together or not at all by
//! [`apply_ops`](domain::ports::DatabasesService::apply_ops). Schema
//! operations (create database/table/column, retype, reorder) are their own
//! structured calls, because property definitions carry configuration DDL
//! cannot express. SQL is not part of the domain: the browser runs the
//! `database_sql` engine itself, and agents run it through the
//! `databases_sql` adapter, both over these same reads and ops.
//!
//! # Architecture
//!
//! - **domain**: models, ports, and the service implementation (all policy).
//! - **inbound**: the Axum router (ops, schema ops, saved queries) and the
//!   schema tools for agents.
//! - **outbound**: Postgres repositories, the cell store over the properties
//!   adapter, and the table-event publisher.

pub mod domain;

#[cfg(any(feature = "inbound", feature = "ai_tools"))]
pub mod inbound;

#[cfg(feature = "outbound")]
pub mod outbound;
