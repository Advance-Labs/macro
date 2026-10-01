#![deny(missing_docs)]
//! The typed write surface of Macro databases.
//!
//! One vocabulary, used as-is in three places: the SQL engine's write step,
//! the body of `POST /databases/{id}/ops`, and the databases domain port that
//! applies it; and the one rule for which type changes keep a column's
//! values. Nothing here knows about SQL.

pub mod cast;
mod ids;
mod ops;

pub use ids::{ColumnId, DatabaseId, RowId, TableId, TableVersion};
pub use ops::{
    CellValue, CellWrite, ColumnKind, DatabaseOp, EntityKind, EntityRef, OpResult, OptionRef,
    RowChange, RowChanges,
};
