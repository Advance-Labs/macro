//! Stage four, and the whole pipeline in one call: [`run`] compiles a
//! statement, fetches through a [`RowSource`] or writes through a
//! [`RowWriter`], folds, and answers with an [`Outcome`].
//!
//! The source and writer are the only I/O; everything else here is pure.
//! Paging and the row cap live here rather than in the source so a fake
//! source can prove them.

#[cfg(test)]
mod test;

use std::future::Future;

use maybe_send::MaybeSend;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::catalog::{Catalog, ColumnKind};
use crate::fold::{Bin, Row, Table, fold_bins, fold_rows};
use crate::resolve::{AggFn, CompileError, Query, SelectItem, SelectQuery, Value, compile};
use crate::split::{GqlQuery, Plan, Shape, split};

/// The most rows one statement reads before the fold. Past it the answer
/// is still returned, marked truncated, so aggregates are visibly partial
/// rather than silently wrong.
pub const ROW_CAP: usize = 20_000;

/// The most rows asked for in one page.
const PAGE_LIMIT: usize = 500;

/// One page of rows from the server.
#[derive(Debug, Clone, PartialEq, Default)]
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
    let columns = describe(catalog, &select.items);
    let plan = split(catalog, select);
    let table = match &plan.gql {
        GqlQuery::Soup { table, .. } | GqlQuery::GroupSoup { table, .. } => *table,
    };

    let (rows, row_ids, truncated) = match &plan.gql {
        GqlQuery::GroupSoup { .. } => {
            let bins = source.bins(&plan.gql).await?;
            (fold_bins(catalog, &plan, bins), Vec::new(), false)
        }
        GqlQuery::Soup { .. } => {
            let (fetched, truncated) = fetch_all(source, &plan).await?;
            let ordered_ids = match &plan.shape {
                Shape::Rows(_) => Some(()),
                Shape::Aggregate { .. } => None,
            };
            let rows = fold_rows(catalog, &plan, fetched.clone());
            let row_ids = match ordered_ids {
                Some(()) => row_ids_in_result_order(catalog, &plan, fetched),
                None => Vec::new(),
            };
            (rows, row_ids, truncated)
        }
    };

    Ok(Outcome {
        columns,
        rows,
        row_ids,
        read_tables: vec![table],
        truncated,
        ..Outcome::default()
    })
}

/// Page through the query until the server runs out or the cap is hit.
async fn fetch_all(source: &impl RowSource, plan: &Plan) -> Result<(Vec<Row>, bool), RunError> {
    let mut rows = Vec::new();
    let mut cursor = None;
    loop {
        let room = ROW_CAP - rows.len();
        if room == 0 {
            return Ok((rows, true));
        }
        let page = source
            .page(&plan.gql, &plan.needs, cursor, room.min(PAGE_LIMIT))
            .await?;
        rows.extend(page.rows);
        if rows.len() > ROW_CAP {
            rows.truncate(ROW_CAP);
            return Ok((rows, true));
        }
        match page.next {
            Some(next) => cursor = Some(next),
            None => return Ok((rows, false)),
        }
    }
}

/// The ids behind a row-shaped result, in the order the fold emits rows.
/// The fold is deterministic, so folding the ids' rows again with a
/// one-column shape yields them in the same order.
fn row_ids_in_result_order(catalog: &Catalog, plan: &Plan, fetched: Vec<Row>) -> Vec<Uuid> {
    let marker = Uuid::nil();
    let tagged: Vec<Row> = fetched
        .into_iter()
        .map(|mut row| {
            row.cells
                .insert(marker, crate::fold::Cell::Text(row.id.to_string()));
            row
        })
        .collect();
    let id_plan = Plan {
        shape: Shape::Rows(vec![marker]),
        ..plan.clone()
    };
    fold_rows(catalog, &id_plan, tagged)
        .into_iter()
        .map(|row| match row.first() {
            Some(Some(crate::fold::Cell::Text(id))) => {
                Uuid::parse_str(id).expect("the marker cell holds the row id")
            }
            _ => unreachable!("every fetched row carries the marker cell"),
        })
        .collect()
}

/// Name and type each select item.
fn describe(catalog: &Catalog, items: &[SelectItem]) -> Vec<OutcomeColumn> {
    let column = |id: Uuid| {
        catalog
            .tables
            .iter()
            .flat_map(|table| &table.columns)
            .find(|column| column.id == id)
            .expect("resolve bound every item to the catalog")
    };
    items
        .iter()
        .map(|item| match item {
            SelectItem::Column(id) => {
                let column = column(*id);
                OutcomeColumn {
                    name: column.name.clone(),
                    column: Some(*id),
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
                column: Some(id),
            } => {
                let column = column(*id);
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
        .collect()
}
