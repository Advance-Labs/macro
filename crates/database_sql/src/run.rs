//! The whole pipeline in one call: [`run`] compiles a statement, drives an
//! [`Engine`] through a [`RowSource`] for its reads and an [`OpsSink`] for its
//! writes, and answers with an [`Outcome`].
//!
//! The source and sink are the only I/O; everything else is pure. Paging
//! and the row cap live in the engine rather than in the source so a fake
//! source can prove them.

#[cfg(test)]
mod test;

use std::future::Future;

use maybe_send::MaybeSend;
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use models_databases::{DatabaseOp, OpResult};

use crate::catalog::{Catalog, ColumnKind};
use crate::engine::{Engine, Step};
use crate::fold::{Bin, Row, Table};
use crate::resolve::{AggFn, Binding, CompileError, Relation, SelectItem};
use crate::split::{GqlQuery, column_of, virtual_column_of};

/// The most rows one statement reads before the fold. Past it the answer
/// is still returned, marked truncated, so aggregates are visibly partial
/// rather than silently wrong.
pub const ROW_CAP: usize = 20_000;

/// The most rows asked for in one page.
pub const PAGE_LIMIT: usize = 500;

/// One page of rows from the server.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    /// The rows, carrying at least the columns asked for.
    pub rows: Vec<Row>,
    /// The cursor for the next page, if there is one.
    pub next: Option<String>,
}

/// Where rows come from: the Soup GraphQL API on the server, the normalized
/// cache in the browser.
pub trait RowSource {
    /// One page of the query, from `cursor` (the start when `None`), at most
    /// `limit` rows. `needs` names the columns the rows must carry.
    fn page(
        &self,
        query: &GqlQuery,
        needs: &[Uuid],
        cursor: Option<String>,
        limit: usize,
    ) -> impl Future<Output = Result<Page, SourceError>> + MaybeSend;

    /// The bins of a `GqlQuery::GroupSoup`.
    fn bins(
        &self,
        query: &GqlQuery,
    ) -> impl Future<Output = Result<Vec<Bin>, SourceError>> + MaybeSend;
}

/// Where writes go: a statement's ops, applied together to one database.
pub trait OpsSink {
    /// Apply `ops` to `database`; one result per op, in order.
    fn apply(
        &self,
        database: Uuid,
        ops: Vec<DatabaseOp>,
    ) -> impl Future<Output = Result<Vec<OpResult>, WriteError>> + MaybeSend;
}

/// A source could not answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SourceError(pub String);

/// A write did not land.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct WriteError(pub String);

/// Why a statement did not run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RunError {
    /// The statement did not compile.
    #[error(transparent)]
    Compile(#[from] CompileError),
    /// The server could not be read.
    #[error("could not read rows: {0}")]
    Source(#[from] SourceError),
    /// A write was refused, in the sink's words.
    #[error(transparent)]
    Write(#[from] WriteError),
    /// An `UPDATE` or `DELETE` named a row by id that the table does not
    /// have.
    #[error("row {}: no row {row} in this table", position + 1)]
    NoSuchRow {
        /// Where the row is named in the statement's list of ids, from 0.
        position: usize,
        /// The row.
        row: Uuid,
    },
    /// An `UPDATE` or `DELETE` matched more rows than a statement reads.
    #[error("the WHERE matches more than {ROW_CAP} rows; narrow it and run the statement again")]
    TooManyRows,
    /// Results were fed that do not answer what was asked.
    #[error("{0}")]
    Results(String),
    /// A feed quoted a request the engine is not waiting on.
    #[error("fed request {fed}, but request {expected} is outstanding")]
    WrongRequest {
        /// The outstanding request.
        expected: u32,
        /// The id fed.
        fed: u32,
    },
    /// A feed arrived when nothing was outstanding.
    #[error("fed request {fed}, but nothing is outstanding")]
    NothingOutstanding {
        /// The id fed.
        fed: u32,
    },
}

/// What a statement produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// The result columns, in select-list order; empty for writes.
    pub columns: Vec<OutcomeColumn>,
    /// The result rows.
    pub rows: Table,
    /// For a row-shaped result, the row entity id behind each result row.
    pub row_ids: Vec<Uuid>,
    /// The tables read, so a caller can watch them for changes.
    pub read_tables: Vec<Uuid>,
    /// Whether the read hit [`ROW_CAP`], making aggregates partial.
    pub truncated: bool,
    /// Rows an `INSERT` created, in statement order for the rows that landed.
    pub inserted_row_ids: Vec<Uuid>,
    /// Rows a write changed.
    pub changes_applied: u32,
    /// The column an `ALTER COLUMN` changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub altered_column: Option<AlteredColumn>,
}

/// A column whose type an `ALTER COLUMN` changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AlteredColumn {
    /// The table.
    pub table: Uuid,
    /// The column's property definition before the change.
    pub column: Uuid,
    /// The type it became, as SQL spells it.
    pub to: String,
    /// Cells `USING NULL` emptied.
    #[specta(type = u32)]
    pub cleared_cells: usize,
    /// Cells that kept only their first of several values.
    #[specta(type = u32)]
    pub trimmed_cells: usize,
}

/// One result column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeColumn {
    /// The name as the statement would call it: the column's display name,
    /// or `SUM(amount)`.
    pub name: String,
    /// The column behind the values, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<Uuid>,
    /// What the values are.
    pub kind: OutcomeKind,
}

/// The value kind of a result column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum OutcomeKind {
    /// Text or link.
    Text,
    /// A number, including every aggregate but `MIN`/`MAX` of dates.
    Number,
    /// A checkbox.
    Boolean,
    /// A date-time.
    Date,
    /// Select option ids; the caller labels them from the catalog.
    Select,
    /// Entity ids; the caller hydrates them.
    Entity,
}

/// Compile and execute one statement.
pub async fn run(
    catalog: &Catalog,
    sql: &str,
    source: &impl RowSource,
    sink: &impl OpsSink,
) -> Result<Outcome, RunError> {
    let (mut engine, mut step) = Engine::start(catalog, sql)?;
    loop {
        step = match step {
            Step::Done(outcome) => return Ok(outcome),
            Step::Fetch(request) => {
                let page = source
                    .page(
                        &request.query,
                        &request.needs,
                        request.cursor,
                        request.limit,
                    )
                    .await?;
                engine.feed_page(request.id, page)?
            }
            Step::Bins(request) => {
                let bins = source.bins(&request.query).await?;
                engine.feed_bins(request.id, bins)?
            }
            Step::Ops { id, database, ops } => {
                let results = sink.apply(database, ops).await?;
                engine.feed_ops(id, results)?
            }
        };
    }
}

/// Name and type each select item.
pub(crate) fn describe(
    catalog: &Catalog,
    items: &[SelectItem],
    labels: &[(usize, String)],
    bindings: &[Binding],
    relations: &[Relation],
) -> Vec<OutcomeColumn> {
    let column = |key: Uuid| {
        column_of(catalog, bindings, relations, key)
            .cloned()
            .or_else(|| virtual_column_of(bindings, relations, key))
            .expect("select items are bound in the scope")
    };
    let mut columns: Vec<OutcomeColumn> = items
        .iter()
        .map(|item| match item {
            SelectItem::Column(key) => {
                let column = column(*key);
                OutcomeColumn {
                    column: bindings
                        .iter()
                        .find(|binding| binding.key == *key)
                        .and_then(|binding| binding.column),
                    kind: match column.kind {
                        ColumnKind::Text | ColumnKind::Link => OutcomeKind::Text,
                        ColumnKind::Number => OutcomeKind::Number,
                        ColumnKind::Boolean => OutcomeKind::Boolean,
                        ColumnKind::Date => OutcomeKind::Date,
                        ColumnKind::Select { .. } => OutcomeKind::Select,
                        ColumnKind::Entity { .. } => OutcomeKind::Entity,
                    },
                    name: column.name,
                }
            }
            SelectItem::Agg { func, column: None } => OutcomeColumn {
                name: format!("{}(*)", func.name()),
                column: None,
                kind: OutcomeKind::Number,
            },
            SelectItem::Agg {
                func,
                column: Some(key),
            } => {
                let column = column(*key);
                OutcomeColumn {
                    name: format!("{}({})", func.name(), column.name),
                    column: None,
                    kind: match (func, &column.kind) {
                        (AggFn::Min | AggFn::Max, ColumnKind::Date) => OutcomeKind::Date,
                        _ => OutcomeKind::Number,
                    },
                }
            }
        })
        .collect();
    for (index, label) in labels {
        if let Some(column) = columns.get_mut(*index) {
            column.name = label.clone();
        }
    }
    columns
}
