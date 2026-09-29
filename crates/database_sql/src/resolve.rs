//! Stage two: bind the parsed AST to a [`Catalog`].
//!
//! Names become ids, literals become typed [`Value`]s, and every comparison,
//! aggregate and ordering is checked against the column it applies to. What
//! comes out is a query later stages can trust without looking at the catalog
//! again. Every failure is a [`ResolveError`] whose message quotes the names
//! the agent wrote.

mod error;
mod filter;
mod insert;
mod names;
mod select;
#[cfg(test)]
mod test;

use chrono::{DateTime, Utc};
use uuid::Uuid;

pub use self::error::ResolveError;
use crate::catalog::Catalog;
use crate::parse::{self, Statement};

pub use crate::parse::{AggFn, CmpOp, Dir};

/// A statement bound to the catalog.
#[derive(Debug, Clone, PartialEq)]
pub enum Query {
    /// A read.
    Select(SelectQuery),
    /// Rows to create.
    Insert(InsertQuery),
}

/// A `SELECT` with every name resolved and every comparison type-checked.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectQuery {
    /// The table read.
    pub table: Uuid,
    /// The select list; `*` has been expanded to every column.
    pub items: Vec<SelectItem>,
    /// The `WHERE` filter.
    pub where_: Option<Filter>,
    /// The `GROUP BY` column.
    pub group_by: Option<Uuid>,
    /// The `ORDER BY` keys, in order.
    pub order_by: Vec<Order>,
}

/// One entry of the select list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectItem {
    /// A column's value.
    Column(Uuid),
    /// An aggregate over a column, or over rows for `COUNT(*)`.
    Agg {
        /// Which aggregate.
        func: AggFn,
        /// The column aggregated; `None` only for `COUNT(*)`.
        column: Option<Uuid>,
    },
}

/// A resolved `WHERE` condition.
#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    /// `column op value`.
    Cmp {
        /// The column.
        column: Uuid,
        /// The operator.
        op: CmpOp,
        /// The value, typed for the column.
        value: Value,
    },
    /// `column [NOT] IN (values)`.
    In {
        /// The column.
        column: Uuid,
        /// The values, typed for the column.
        values: Vec<Value>,
        /// `NOT IN`.
        negated: bool,
    },
    /// `column [NOT] HAS value` on a multi-valued column.
    Has {
        /// The column.
        column: Uuid,
        /// The member tested.
        value: Value,
        /// `NOT HAS`.
        negated: bool,
    },
    /// `column IS [NOT] NULL`.
    IsNull {
        /// The column.
        column: Uuid,
        /// `IS NOT NULL`.
        negated: bool,
    },
    /// `column [NOT] LIKE pattern` on a text column.
    Like {
        /// The column.
        column: Uuid,
        /// The pattern, with SQL `%` and `_` wildcards.
        pattern: String,
        /// `NOT LIKE`.
        negated: bool,
    },
    /// All must hold.
    And(Vec<Filter>),
    /// Any must hold.
    Or(Vec<Filter>),
}

/// A literal after it has been typed for the column it is compared to or
/// stored in.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Text, on text and link columns.
    Text(String),
    /// A number.
    Number(f64),
    /// A checkbox state.
    Bool(bool),
    /// A date-time, parsed from an ISO 8601 date or date-time literal.
    Date(DateTime<Utc>),
    /// A select option, resolved from its label.
    Option(Uuid),
    /// An entity id such as `macro|sam@example.com`.
    Entity(String),
}

/// One `ORDER BY` key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order {
    /// What is sorted on.
    pub key: OrderKey,
    /// The direction.
    pub dir: Dir,
}

/// What an `ORDER BY` key refers to after resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderKey {
    /// A column, selected or not.
    Column(Uuid),
    /// A 0-based index into the select list; positional keys and aggregates
    /// both resolve to this.
    Item(usize),
}

/// An `INSERT` with columns resolved and values typed.
#[derive(Debug, Clone, PartialEq)]
pub struct InsertQuery {
    /// The table written.
    pub table: Uuid,
    /// One entry per row: the cells to set, in column-list order. A `NULL`
    /// literal is not a cell.
    pub rows: Vec<Vec<(Uuid, Value)>>,
}

/// Bind a parsed statement to the catalog.
pub fn resolve(catalog: &Catalog, statement: Statement) -> Result<Query, ResolveError> {
    match statement {
        Statement::Select(select) => {
            let table = names::table(catalog, &select.table)?;
            select::resolve(table, select).map(Query::Select)
        }
        Statement::Insert(insert) => {
            let table = names::table(catalog, &insert.table)?;
            insert::resolve(table, insert).map(Query::Insert)
        }
    }
}

/// Parse and resolve in one step.
pub fn compile(catalog: &Catalog, sql: &str) -> Result<Query, CompileError> {
    let statement = parse::parse(sql)?;
    Ok(resolve(catalog, statement)?)
}

/// Why a statement could not be compiled.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CompileError {
    /// The text was not a statement of the supported grammar.
    #[error(transparent)]
    Parse(#[from] parse::ParseError),
    /// The statement named something the catalog does not have, or used a
    /// column in a way its type does not allow.
    #[error(transparent)]
    Resolve(#[from] ResolveError),
}
