#![deny(missing_docs)]
//! The typed write surface of Macro databases.
//!
//! One vocabulary, used as-is in three places: the SQL engine's write step,
//! the body of `POST /databases/{id}/ops`, and the databases domain port that
//! applies it; the one rule for which type changes keep a column's values;
//! the typed views of a table and the rule for a board's card order; and
//! the fractional keys everything is ordered by. Nothing here knows about
//! SQL.

pub mod cast;
mod ids;
mod ops;
pub mod position;
pub mod views;

pub use ids::{ColumnId, DatabaseId, OptionId, RowId, TableId, TableVersion};
pub use ops::{
    CellValue, CellWrite, ColumnKind, DatabaseOp, EntityKind, EntityRef, OpResult, OptionRef,
    RowChange, RowChanges,
};
pub use option_palette::OptionColor;
