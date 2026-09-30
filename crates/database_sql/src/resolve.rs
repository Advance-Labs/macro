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
pub use self::names::ROW_ID;
use crate::catalog::Catalog;
use crate::parse::{self, Statement};

pub use crate::parse::{AggFn, CmpOp, Dir, JoinKind};

/// A statement bound to the catalog.
#[derive(Debug, Clone, PartialEq)]
pub enum Query {
    /// A read.
    Select(SelectQuery),
    /// Rows to create.
    Insert(InsertQuery),
    /// Cells to set on one row.
    Update(UpdateQuery),
    /// One row to remove.
    Delete(DeleteQuery),
}

/// A `SELECT` with every name resolved and every comparison type-checked.
///
/// Columns are referred to by *key*, not by property definition id: one
/// definition can be bound to several of the joined tables, so a key names
/// a column of one relation. See [`column_key`].
#[derive(Debug, Clone, PartialEq)]
pub struct SelectQuery {
    /// `DISTINCT`: repeated result rows are dropped.
    pub distinct: bool,
    /// The tables read: the `FROM` table first, then each join's.
    pub relations: Vec<Relation>,
    /// The joins, in statement order; `joins[i]` brings in `relations[i + 1]`.
    pub joins: Vec<ResolvedJoin>,
    /// The select list; `*` has been expanded to every column.
    pub items: Vec<SelectItem>,
    /// The names select-list items were given with `AS`, by position.
    pub labels: Vec<(usize, String)>,
    /// The `WHERE` filter.
    pub where_: Option<Filter>,
    /// The `GROUP BY` column.
    pub group_by: Option<Uuid>,
    /// The `ORDER BY` keys, in order.
    pub order_by: Vec<Order>,
    /// `LIMIT`: at most this many result rows.
    pub limit: Option<u32>,
    /// `OFFSET`: skip this many result rows first.
    pub offset: Option<u32>,
    /// What every key the query mentions refers to.
    pub bindings: Vec<Binding>,
}

/// One table read by a `SELECT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    /// The table.
    pub table: Uuid,
    /// The alias its columns are qualified by.
    pub alias: String,
}

/// A join, resolved: each `on` pair is (a key of an earlier relation, a key
/// of the joined relation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedJoin {
    /// The relation joined in.
    pub relation: usize,
    /// Inner or left.
    pub kind: JoinKind,
    /// The equalities, all of which must hold.
    pub on: Vec<(Uuid, Uuid)>,
}

/// What a key refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// The key.
    pub key: Uuid,
    /// The relation the column belongs to.
    pub relation: usize,
    /// The property definition; `None` for the row id.
    pub column: Option<Uuid>,
}

/// The key of a column: the definition id itself in the `FROM` table, so a
/// single-table query is keyed exactly as before, and a name derived from it
/// in each joined table.
pub fn column_key(relation: usize, column: Uuid) -> Uuid {
    if relation == 0 {
        column
    } else {
        Uuid::new_v5(&column, &[relation as u8])
    }
}

/// The key of a table's row id.
pub fn row_id_key(table: Uuid) -> Uuid {
    Uuid::new_v5(&table, b"row_id")
}

impl SelectQuery {
    /// The `FROM` table.
    pub fn table(&self) -> Uuid {
        self.relations[0].table
    }

    /// What a key refers to, if the query mentions it.
    pub fn binding(&self, key: Uuid) -> Option<&Binding> {
        self.bindings.iter().find(|binding| binding.key == key)
    }
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
        /// The character that makes the next pattern character literal.
        escape: Option<char>,
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
    /// Every option of a multi-select cell being written.
    Options(Vec<Uuid>),
    /// Every reference of a multi-valued entity cell being written.
    Entities(Vec<String>),
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
    /// A column key, selected or not.
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

/// An `UPDATE` of one row with every cell typed.
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateQuery {
    /// The table written.
    pub table: Uuid,
    /// The row.
    pub row_id: Uuid,
    /// The cells to set, in statement order; `None` clears the cell.
    pub cells: Vec<(Uuid, Option<Value>)>,
}

/// A `DELETE` of one row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteQuery {
    /// The table written.
    pub table: Uuid,
    /// The row.
    pub row_id: Uuid,
}

/// Bind a parsed statement to the catalog.
pub fn resolve(catalog: &Catalog, statement: Statement) -> Result<Query, ResolveError> {
    match statement {
        Statement::Select(select) => select::resolve(catalog, select).map(Query::Select),
        Statement::Insert(insert) => {
            let table = names::table(catalog, &insert.table)?;
            insert::resolve(table, insert).map(Query::Insert)
        }
        Statement::Update(update) => {
            let table = names::table(catalog, &update.table)?;
            insert::resolve_update(table, update).map(Query::Update)
        }
        Statement::Delete(delete) => {
            let table = names::table(catalog, &delete.table)?;
            Ok(Query::Delete(DeleteQuery {
                table: table.id,
                row_id: insert::row_id(&delete.row_id)?,
            }))
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
