//! The subset AST. Untyped and unresolved: names are [`Ident`]s and values
//! are [`Lit`]s; binding them to a catalog is the next stage's job. No spans:
//! later stages report problems by quoting the identifier.

/// An identifier as written, quotes removed, case preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident(pub String);

/// One parsed statement.
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    /// A `SELECT`.
    Select(Select),
    /// An `INSERT … VALUES`.
    Insert(Insert),
    /// An `UPDATE … SET … WHERE row_id = …`.
    Update(Update),
    /// A `DELETE FROM … WHERE row_id = …`.
    Delete(Delete),
}

/// `SELECT items FROM table [WHERE] [GROUP BY] [ORDER BY]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Select {
    /// The select list.
    pub items: Vec<Item>,
    /// The single table read.
    pub table: TableName,
    /// The `WHERE` condition.
    pub where_: Option<Cond>,
    /// The `GROUP BY` column.
    pub group_by: Option<Ident>,
    /// The `ORDER BY` keys, in order.
    pub order_by: Vec<OrderBy>,
}

/// `[database.]table`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableName {
    /// The database, when qualified.
    pub database: Option<Ident>,
    /// The table.
    pub table: Ident,
}

/// One entry of the select list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// `*`.
    Star,
    /// A column.
    Column(Ident),
    /// An aggregate call.
    Agg(Agg),
}

/// An aggregate call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agg {
    /// Which aggregate.
    pub func: AggFn,
    /// The column aggregated; `None` only for `COUNT(*)`.
    pub arg: Option<Ident>,
}

/// The aggregate functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggFn {
    /// `COUNT`.
    Count,
    /// `SUM`.
    Sum,
    /// `AVG`.
    Avg,
    /// `MIN`.
    Min,
    /// `MAX`.
    Max,
}

/// A `WHERE` condition.
#[derive(Debug, Clone, PartialEq)]
pub enum Cond {
    /// `column op value`.
    Cmp {
        /// The column.
        column: Ident,
        /// The operator.
        op: CmpOp,
        /// The literal compared against.
        value: Lit,
    },
    /// `column [NOT] IN (values)`.
    In {
        /// The column.
        column: Ident,
        /// The literals listed.
        values: Vec<Lit>,
        /// `NOT IN`.
        negated: bool,
    },
    /// `column [NOT] HAS value`: membership in a multi-valued column.
    Has {
        /// The column.
        column: Ident,
        /// The member tested.
        value: Lit,
        /// `NOT HAS`.
        negated: bool,
    },
    /// `column IS [NOT] NULL`.
    IsNull {
        /// The column.
        column: Ident,
        /// `IS NOT NULL`.
        negated: bool,
    },
    /// `column [NOT] LIKE pattern`.
    Like {
        /// The column.
        column: Ident,
        /// The pattern, with SQL `%` and `_` wildcards.
        pattern: String,
        /// `NOT LIKE`.
        negated: bool,
    },
    /// Two or more conditions joined by `AND`.
    And(Vec<Cond>),
    /// Two or more conditions joined by `OR`.
    Or(Vec<Cond>),
}

/// A comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    /// `=`.
    Eq,
    /// `!=` or `<>`.
    Ne,
    /// `<`.
    Lt,
    /// `<=`.
    Le,
    /// `>`.
    Gt,
    /// `>=`.
    Ge,
}

/// A literal value.
#[derive(Debug, Clone, PartialEq)]
pub enum Lit {
    /// `'text'`.
    Str(String),
    /// A number.
    Num(f64),
    /// `TRUE` or `FALSE`.
    Bool(bool),
    /// `NULL`.
    Null,
}

/// One `ORDER BY` key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderBy {
    /// What is sorted on.
    pub key: OrderKey,
    /// The direction; `ASC` when unspecified.
    pub dir: Dir,
}

/// What an `ORDER BY` key refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderKey {
    /// A column.
    Column(Ident),
    /// An aggregate that also appears in the select list.
    Agg(Agg),
    /// A 1-based position in the select list.
    Position(u32),
}

/// A sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// Ascending.
    Asc,
    /// Descending.
    Desc,
}

/// `INSERT INTO table (columns) VALUES rows`.
#[derive(Debug, Clone, PartialEq)]
pub struct Insert {
    /// The table written.
    pub table: TableName,
    /// The columns named, in order.
    pub columns: Vec<Ident>,
    /// The rows; every row has exactly `columns.len()` values.
    pub rows: Vec<Vec<Lit>>,
}

/// `UPDATE table SET column = value, … WHERE row_id = 'id'`: one row, by id.
#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    /// The table written.
    pub table: TableName,
    /// The cells set, in order; a `NULL` value clears the cell.
    pub assignments: Vec<(Ident, Lit)>,
    /// The row, as written in the `WHERE`.
    pub row_id: String,
}

/// `DELETE FROM table WHERE row_id = 'id'`: one row, by id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delete {
    /// The table written.
    pub table: TableName,
    /// The row, as written in the `WHERE`.
    pub row_id: String,
}
