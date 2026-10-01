//! Identifiers of a database's parts.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Identifier of a database (the shareable entity users see).
pub type DatabaseId = Uuid;
/// Identifier of one table (tab) within a database.
pub type TableId = Uuid;
/// Identifier of a column placement within a table.
pub type ColumnId = Uuid;
/// Identifier of a row.
pub type RowId = Uuid;
/// Identifier of an option of a select or tag column.
pub type OptionId = Uuid;

/// Monotonic per-table version, bumped on every row/column/link mutation.
///
/// The cache key for query materializations and the invalidation signal for
/// live query chips.
#[derive(
    utoipa::ToSchema,
    specta::Type,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
)]
// A version never nears 2^53, so TypeScript reads it as a plain number.
#[specta(type = f64)]
pub struct TableVersion(pub i64);
