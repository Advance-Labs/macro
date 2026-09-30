//! The subset AST. Untyped and unresolved: names are [`Ident`]s and values
//! are [`Lit`]s; binding them to a catalog is the next stage's job. No spans:
//! later stages report problems by quoting the identifier.

use crate::cast::ColumnType;

/// An identifier as written, quotes removed, case preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident(pub String);

/// One parsed statement.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    not(target_arch = "wasm32"),
    expect(
        clippy::large_enum_variant,
        reason = "a statement is parsed once and moved once; boxing the select would only add noise to every test literal"
    )
)]
pub enum Statement {
    /// A `SELECT`.
    Select(Select),
    /// An `INSERT … VALUES`.
    Insert(Insert),
    /// An `UPDATE … SET … WHERE row_id = …`.
    Update(Update),
    /// A `DELETE FROM … WHERE row_id = …`.
    Delete(Delete),
    /// An `ALTER TABLE … ALTER COLUMN … TYPE …`.
    AlterColumnType(AlterColumnType),
}

/// `SELECT [DISTINCT] items FROM table [JOIN …] [WHERE] [GROUP BY] [ORDER BY]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Select {
    /// `DISTINCT`: drop repeated result rows.
    pub distinct: bool,
    /// The select list.
    pub items: Vec<Item>,
    /// `item AS name`: the select-list position and the name it goes by.
    pub aliases: Vec<(usize, Ident)>,
    /// The table the `FROM` names.
    pub from: FromItem,
    /// The joined tables, in statement order.
    pub joins: Vec<Join>,
    /// The `WHERE` condition.
    pub where_: Option<Cond>,
    /// The `GROUP BY` column.
    pub group_by: Option<ColumnRef>,
    /// The `ORDER BY` keys, in order.
    pub order_by: Vec<OrderBy>,
    /// `LIMIT n`.
    pub limit: Option<u32>,
    /// `OFFSET n`.
    pub offset: Option<u32>,
}

/// A table read, with the alias its columns are qualified by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FromItem {
    /// The table.
    pub table: TableName,
    /// `[AS] alias`; without one, the table name qualifies its columns.
    pub alias: Option<Ident>,
}

/// `JOIN table ON left = right [AND left = right]…`.
#[derive(Debug, Clone, PartialEq)]
pub struct Join {
    /// Inner or left.
    pub kind: JoinKind,
    /// The table joined in.
    pub table: FromItem,
    /// The equalities the joined rows must satisfy, all of them.
    pub on: Vec<(ColumnRef, ColumnRef)>,
}

/// How unmatched rows are treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinKind {
    /// Only rows with a match on both sides.
    Inner,
    /// Every row of the earlier tables, matched or not.
    Left,
}

/// A column as written: `column` or `alias.column`. The name `row_id` refers
/// to a table's row entity id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnRef {
    /// The alias qualifying the column, if any.
    pub table: Option<Ident>,
    /// The column.
    pub column: Ident,
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
    Column(ColumnRef),
    /// An aggregate call.
    Agg(Agg),
}

/// An aggregate call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agg {
    /// Which aggregate.
    pub func: AggFn,
    /// The column aggregated; `None` only for `COUNT(*)`.
    pub arg: Option<ColumnRef>,
}

/// The aggregate functions; the string form is the name as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "UPPERCASE")]
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
        column: ColumnRef,
        /// The operator.
        op: CmpOp,
        /// The literal compared against.
        value: Lit,
    },
    /// `column [NOT] IN (values)`.
    In {
        /// The column.
        column: ColumnRef,
        /// The literals listed.
        values: Vec<Lit>,
        /// `NOT IN`.
        negated: bool,
    },
    /// `column [NOT] HAS value`: membership in a multi-valued column.
    Has {
        /// The column.
        column: ColumnRef,
        /// The member tested.
        value: Lit,
        /// `NOT HAS`.
        negated: bool,
    },
    /// `column IS [NOT] NULL`.
    IsNull {
        /// The column.
        column: ColumnRef,
        /// `IS NOT NULL`.
        negated: bool,
    },
    /// `column [NOT] LIKE pattern`.
    Like {
        /// The column.
        column: ColumnRef,
        /// The pattern, with SQL `%` and `_` wildcards.
        pattern: String,
        /// `ESCAPE 'c'`: the character that makes the next pattern character
        /// literal.
        escape: Option<char>,
        /// `NOT LIKE`.
        negated: bool,
    },
    /// Two or more conditions joined by `AND`.
    And(Vec<Cond>),
    /// Two or more conditions joined by `OR`.
    Or(Vec<Cond>),
}

/// A comparison operator; the string form is the symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr)]
pub enum CmpOp {
    /// `=`.
    #[strum(serialize = "=")]
    Eq,
    /// `!=` or `<>`.
    #[strum(serialize = "!=")]
    Ne,
    /// `<`.
    #[strum(serialize = "<")]
    Lt,
    /// `<=`.
    #[strum(serialize = "<=")]
    Le,
    /// `>`.
    #[strum(serialize = ">")]
    Gt,
    /// `>=`.
    #[strum(serialize = ">=")]
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
    /// `[value, …]`: several values for a multi-valued cell. Only in
    /// `INSERT` rows and `UPDATE` assignments.
    List(Vec<Lit>),
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
    Column(ColumnRef),
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
    /// The columns named, in order; empty for `DEFAULT VALUES`.
    pub columns: Vec<Ident>,
    /// The rows; every row has exactly `columns.len()` values. `DEFAULT
    /// VALUES` is one empty row.
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

/// `ALTER TABLE table ALTER [COLUMN] column TYPE type [USING NULL]`: change
/// one column's type, converting its values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlterColumnType {
    /// The table whose column changes.
    pub table: TableName,
    /// The column.
    pub column: Ident,
    /// The type it becomes.
    pub to: ColumnType,
    /// `USING NULL`: empty the values that do not fit instead of refusing.
    pub clear_invalid: bool,
}
