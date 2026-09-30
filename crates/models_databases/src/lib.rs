#![deny(missing_docs)]
//! The typed write surface of Macro databases.
//!
//! One vocabulary, used as-is in three places: the SQL engine's write step,
//! the body of `POST /databases/{id}/ops`, and the databases domain port that
//! applies it. Nothing here knows about SQL.

mod ids;
mod ops;

pub use ids::{ColumnId, DatabaseId, RowId, TableId, TableVersion};
pub use ops::{
    CellValue, CellWrite, ColumnKind, DatabaseOp, EntityKind, EntityRef, OpResult, OptionRef,
    RowChange, RowChanges,
};
