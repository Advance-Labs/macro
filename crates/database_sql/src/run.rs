//! The whole pipeline in one call: [`run`] compiles a statement, drives an
//! [`Engine`] through a [`RowSource`] or writes through a [`RowWriter`], and
//! answers with an [`Outcome`].
//!
//! The source and writer are the only I/O; everything else is pure. Paging
//! and the row cap live in the engine rather than in the source so a fake
//! source can prove them.

#[cfg(test)]
mod test;

use std::future::Future;

use maybe_send::MaybeSend;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::catalog::{Catalog, ColumnKind};
use crate::engine::{Engine, Step};
use crate::fold::{Bin, Row, Table};
use crate::resolve::{
    AggFn, Binding, CompileError, Query, Relation, SelectItem, SelectQuery, Value, compile,
};
use crate::split::{GqlQuery, column_of};

/// The most rows one statement reads before the fold. Past it the answer
/// is still returned, marked truncated, so aggregates are visibly partial
/// rather than silently wrong.
pub const ROW_CAP: usize = 20_000;

/// The most rows asked for in one page.
pub const PAGE_LIMIT: usize = 500;

/// One page of rows from the server.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
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

/// Where writes go: the properties service on the server, the optimistic
/// mutation queue in the browser. Each call is one row.
pub trait RowWriter {
    /// Create a row with these cells; answers the new row id.
    fn insert(
        &self,
        table: Uuid,
        cells: Vec<(Uuid, Value)>,
    ) -> impl Future<Output = Result<Uuid, WriteError>> + MaybeSend;

    /// Set (or, with `None`, clear) these cells on a row.
    fn update(
        &self,
        table: Uuid,
        row_id: Uuid,
        cells: Vec<(Uuid, Option<Value>)>,
    ) -> impl Future<Output = Result<(), WriteError>> + MaybeSend;

    /// Remove a row.
    fn delete(
        &self,
        table: Uuid,
        row_id: Uuid,
    ) -> impl Future<Output = Result<(), WriteError>> + MaybeSend;
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
    /// The engine was started on a write.
    #[error("the engine runs SELECT statements; writes go through run()")]
    NotARead,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
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
    /// Rows a write could not change, by statement position.
    pub failures: Vec<RowFailure>,
}

/// One result column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeColumn {
    /// The name as the statement would call it: the column's display name,
    /// or `SUM(amount)`.
    pub name: String,
    /// The column behind the values, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<Uuid>,
    /// What the values are.
    pub kind: OutcomeKind,
}

/// The value kind of a result column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// A write that did not land on one row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowFailure {
    /// 0-based position of the row in the statement; for `UPDATE` and
    /// `DELETE`, always 0.
    pub row: usize,
    /// Why, as the writer said it.
    pub message: String,
}

/// Compile and execute one statement.
pub async fn run(
    catalog: &Catalog,
    sql: &str,
    source: &impl RowSource,
    writer: &impl RowWriter,
) -> Result<Outcome, RunError> {
    match compile(catalog, sql)? {
        Query::Select(select) => read(catalog, select, source).await,
        Query::Insert(insert) => {
            let mut outcome = Outcome::default();
            for (position, cells) in insert.rows.into_iter().enumerate() {
                match writer.insert(insert.table, cells).await {
                    Ok(row_id) => {
                        outcome.inserted_row_ids.push(row_id);
                        outcome.changes_applied += 1;
                    }
                    Err(WriteError(message)) => outcome.failures.push(RowFailure {
                        row: position,
                        message,
                    }),
                }
            }
            Ok(outcome)
        }
        Query::Update(update) => Ok(one_row(
            writer
                .update(update.table, update.row_id, update.cells)
                .await,
        )),
        Query::Delete(delete) => Ok(one_row(writer.delete(delete.table, delete.row_id).await)),
    }
}

/// The outcome of a single-row write.
fn one_row(result: Result<(), WriteError>) -> Outcome {
    match result {
        Ok(()) => Outcome {
            changes_applied: 1,
            ..Outcome::default()
        },
        Err(WriteError(message)) => Outcome {
            failures: vec![RowFailure { row: 0, message }],
            ..Outcome::default()
        },
    }
}

async fn read(
    catalog: &Catalog,
    select: SelectQuery,
    source: &impl RowSource,
) -> Result<Outcome, RunError> {
    let (mut engine, mut step) = Engine::from_select(catalog, select);
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
    let column = |key: Uuid| column_of(catalog, bindings, relations, key);
    let mut columns: Vec<OutcomeColumn> = items
        .iter()
        .map(|item| match item {
            SelectItem::Column(key) => {
                let Some(column) = column(*key) else {
                    return OutcomeColumn {
                        name: crate::resolve::ROW_ID.into(),
                        column: None,
                        kind: OutcomeKind::Entity,
                    };
                };
                OutcomeColumn {
                    name: column.name.clone(),
                    column: Some(column.id),
                    kind: match column.kind {
                        ColumnKind::Text | ColumnKind::Link => OutcomeKind::Text,
                        ColumnKind::Number => OutcomeKind::Number,
                        ColumnKind::Boolean => OutcomeKind::Boolean,
                        ColumnKind::Date => OutcomeKind::Date,
                        ColumnKind::Select { .. } => OutcomeKind::Select,
                        ColumnKind::Entity { .. } => OutcomeKind::Entity,
                    },
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
                let (name, kind) = match column(*key) {
                    Some(column) => (column.name.clone(), column.kind.clone()),
                    None => (
                        crate::resolve::ROW_ID.into(),
                        ColumnKind::Entity { multi: false },
                    ),
                };
                OutcomeColumn {
                    name: format!("{}({})", func.name(), name),
                    column: None,
                    kind: match (func, &kind) {
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
