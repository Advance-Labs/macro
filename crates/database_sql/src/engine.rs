//! Stage four as a state machine: an [`Engine`] carries one statement from
//! its first step to its [`Outcome`], asking the driver for rows one
//! [`Request`] at a time (taken back through [`Engine::feed_page`] and
//! [`Engine::feed_bins`]) and for a write to be applied as ops (taken back
//! through [`Engine::feed_ops`]).
//!
//! The engine does no I/O and holds no references, so it crosses a wasm
//! boundary as JSON: a driver in any language loops on [`Step`] until it is
//! [`Step::Done`]. Relations are fetched in plan order, each to completion,
//! and a joined relation's request carries the key values the rows so far
//! need (see [`KeyHint`]), which a driver may use to fetch less.
//!
//! An `INSERT` or `ALTER COLUMN` is one [`Step::Ops`]. An `UPDATE` or
//! `DELETE` first reads the rows its `WHERE` matches, through the same fetch
//! steps as a `SELECT`, then writes them all in one op.

#[cfg(test)]
mod test;

use models_databases::{DatabaseOp, OpResult};
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::catalog::{Catalog, Table};
use crate::fold::{Bin, Cell, Row, fold_bins, fold_relations};
use crate::resolve::{
    ComparisonOperator, DeleteQuery, Filter, Query, SelectQuery, UpdateQuery, Value, binding,
    column_key, compile, row_id_key, row_position_key,
};
use crate::run::{Outcome, OutcomeColumn, PAGE_LIMIT, Page, ROW_CAP, RunError, describe};
use crate::split::{GqlQuery, KeyHint, Plan, Shape, split};
use crate::write::{self, Sent};

/// What the driver does next.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "step", rename_all = "camelCase")]
pub enum Step {
    /// Fetch one page and feed it to [`Engine::feed_page`].
    Fetch(Request),
    /// Fetch the bins of a `GroupSoup` query and feed them to
    /// [`Engine::feed_bins`].
    Bins(Request),
    /// Apply these ops to the database, together, and feed their results to
    /// [`Engine::feed_ops`].
    Ops {
        /// Identifies the request; the feed must quote it.
        id: u32,
        /// The database the ops are for.
        database: Uuid,
        /// The ops, in order.
        ops: Vec<DatabaseOp>,
    },
    /// The answer.
    Done(Outcome),
}

/// One fetch the engine wants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    /// Identifies the request; the feed must quote it.
    pub id: u32,
    /// What to ask the server.
    pub query: GqlQuery,
    /// The keys the rows must carry; the driver maps them to properties
    /// through the plan's bindings, or fetches every column.
    pub needs: Vec<Uuid>,
    /// Where to continue; `None` at the start of a relation.
    pub cursor: Option<String>,
    /// At most this many rows.
    #[specta(type = u32)]
    pub limit: usize,
}

/// One statement in flight.
#[derive(Debug, Clone)]
pub struct Engine {
    catalog: Catalog,
    /// The `SELECT` being fetched: the statement's own, or the one that
    /// finds an update's or delete's rows.
    read: Option<Read>,
    /// What the statement writes once its read is done.
    then: Option<Then>,
    /// The ops sent, whose results make the outcome.
    sent: Option<Sent>,
    requests: Requests,
}

/// The ids handed to requests, and the one a feed must answer.
#[derive(Debug, Clone, Default)]
struct Requests {
    outstanding: Option<u32>,
    next: u32,
}

/// A read's progress.
#[derive(Debug, Clone)]
struct Read {
    columns: Vec<OutcomeColumn>,
    plan: Plan,
    /// The rows of every relation fetched so far, `FROM` first.
    fetched: Vec<Vec<Row>>,
    /// The relation being fetched.
    current: usize,
    truncated: bool,
}

/// A write waiting on the rows its read finds.
#[derive(Debug, Clone)]
enum Then {
    Update(UpdateQuery),
    Delete(DeleteQuery),
}

impl Engine {
    /// Compile a statement and take its first step.
    pub fn start(catalog: &Catalog, sql: &str) -> Result<(Engine, Step), RunError> {
        let mut engine = Engine::new(catalog);
        let step = match compile(catalog, sql)? {
            Query::Select(select) => engine.begin_read(select),
            Query::Insert(insert) => {
                let op = write::insert(catalog, &insert);
                engine.send(insert.table, Sent::Rows, op)
            }
            Query::AlterColumnType(alter) => {
                let op = write::alter(catalog, &alter);
                let sent = Sent::Column {
                    table: alter.table,
                    column: alter.column,
                    to: alter.to,
                };
                engine.send(alter.table, sent, op)
            }
            Query::Update(update) => {
                let read = update.read.clone();
                engine.then = Some(Then::Update(update));
                engine.begin_read(read)
            }
            Query::Delete(delete) => {
                let read = delete.read.clone();
                engine.then = Some(Then::Delete(delete));
                engine.begin_read(read)
            }
        };
        Ok((engine, step))
    }

    /// Plan a resolved `SELECT` and ask for its first fetch.
    pub fn from_select(catalog: &Catalog, select: SelectQuery) -> (Engine, Step) {
        let mut engine = Engine::new(catalog);
        let step = engine.begin_read(select);
        (engine, step)
    }

    fn new(catalog: &Catalog) -> Engine {
        Engine {
            catalog: catalog.clone(),
            read: None,
            then: None,
            sent: None,
            requests: Requests::default(),
        }
    }

    /// The plan of the read in flight, if the statement reads.
    pub fn plan(&self) -> Option<&Plan> {
        self.read.as_ref().map(|read| &read.plan)
    }

    /// Take one page of the outstanding request. Cells may be keyed by
    /// property definition, as a source reads them; the engine keys a
    /// joined relation's cells itself (see [`column_key`]).
    pub fn feed_page(&mut self, request_id: u32, page: Page) -> Result<Step, RunError> {
        self.requests.answer(request_id)?;
        let read = reading(&mut self.read)?;
        let keyed = keyed(&self.catalog, read, page.rows);
        let rows = &mut read.fetched[read.current];
        rows.extend(keyed);
        if rows.len() >= ROW_CAP {
            rows.truncate(ROW_CAP);
            read.truncated = true;
            return self.finish_relation();
        }
        match page.next {
            Some(cursor) => Ok(request(read, &mut self.requests, Some(cursor))),
            None => self.finish_relation(),
        }
    }

    /// Take the bins of the outstanding request.
    pub fn feed_bins(&mut self, request_id: u32, bins: Vec<Bin>) -> Result<Step, RunError> {
        self.requests.answer(request_id)?;
        let read = reading(&mut self.read)?;
        let rows = fold_bins(&self.catalog, &read.plan, bins);
        let outcome = read.outcome(rows, Vec::new());
        self.finish_read(outcome)
    }

    /// Take the results of the outstanding ops, one per op.
    pub fn feed_ops(&mut self, request_id: u32, results: Vec<OpResult>) -> Result<Step, RunError> {
        self.requests.answer(request_id)?;
        let sent = self
            .sent
            .take()
            .ok_or(RunError::NothingOutstanding { fed: request_id })?;
        Ok(Step::Done(write::outcome(&sent, &results)?))
    }

    /// Plan a read and ask for its first fetch.
    fn begin_read(&mut self, select: SelectQuery) -> Step {
        let columns = describe(
            &self.catalog,
            &select.items,
            &select.labels,
            &select.bindings,
            &select.relations,
        );
        let plan = split(&self.catalog, select);
        let read = self.read.insert(Read {
            columns,
            plan,
            fetched: Vec::new(),
            current: 0,
            truncated: false,
        });
        begin_relation(read, &mut self.requests)
    }

    /// A relation is complete: give it its row ids and positions, then move
    /// on or fold.
    fn finish_relation(&mut self) -> Result<Step, RunError> {
        let read = reading(&mut self.read)?;
        let table = read.plan.relations[read.current].relation.table;
        let bound = |key: Uuid| binding(&read.plan.bindings, key).is_some();
        let (id_key, position_key) = (row_id_key(table), row_position_key(table));
        let (ids, positions) = (bound(id_key), bound(position_key));
        // Sources list rows in any order (Soup: newest first); the fold sees
        // them in table order, as the server's row store keeps them, so an
        // unordered result or its groups come out alike on both.
        read.fetched[read.current].sort_by(table_order);
        for row in &mut read.fetched[read.current] {
            if ids {
                row.cells
                    .insert(id_key, Cell::Entities(vec![row.id.to_string()]));
            }
            if positions && let Some(position) = &row.position {
                row.cells.insert(position_key, Cell::Text(position.clone()));
            }
        }
        read.current += 1;
        if read.current < read.plan.relations.len() {
            return Ok(begin_relation(read, &mut self.requests));
        }
        let fetched = std::mem::take(&mut read.fetched);
        let (rows, row_ids) = fold_relations(&self.catalog, &read.plan, fetched);
        let row_ids = match read.plan.shape {
            Shape::Rows(_) => row_ids,
            Shape::Aggregate { .. } => Vec::new(),
        };
        let outcome = read.outcome(rows, row_ids);
        self.finish_read(outcome)
    }

    /// The read's answer, or for a write, the op it makes of the rows found.
    fn finish_read(&mut self, found: Outcome) -> Result<Step, RunError> {
        let Some(then) = self.then.take() else {
            return Ok(Step::Done(found));
        };
        if found.truncated {
            return Err(RunError::TooManyRows { limit: ROW_CAP });
        }
        let read = match &then {
            Then::Update(update) => &update.read,
            Then::Delete(delete) => &delete.read,
        };
        if let Some((position, row)) = named_rows(read)
            .into_iter()
            .enumerate()
            .find(|(_, row)| !found.row_ids.contains(row))
        {
            return Err(RunError::NoSuchRow {
                position: position + 1,
                row,
            });
        }
        let (table, op) = match &then {
            Then::Update(update) => (update.table, write::update(&self.catalog, update, &found)),
            Then::Delete(delete) => (delete.table, write::delete(delete.table, &found)),
        };
        Ok(match op {
            Some(op) => self.send(table, Sent::Rows, op),
            None => Step::Done(Outcome::default()),
        })
    }

    /// Ask for one op to be applied to the database `table` belongs to.
    fn send(&mut self, table_id: Uuid, sent: Sent, op: DatabaseOp) -> Step {
        let database = table(&self.catalog, table_id).database_id;
        self.sent = Some(sent);
        Step::Ops {
            id: self.requests.issue(),
            database,
            ops: vec![op],
        }
    }
}

impl Requests {
    /// A new request's id, now the one outstanding.
    fn issue(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        self.outstanding = Some(id);
        id
    }

    /// A feed for `request_id`, which must be the outstanding request.
    fn answer(&mut self, request_id: u32) -> Result<(), RunError> {
        match self.outstanding.take() {
            Some(expected) if expected == request_id => Ok(()),
            Some(expected) => Err(RunError::WrongRequest {
                expected,
                fed: request_id,
            }),
            None => Err(RunError::NothingOutstanding { fed: request_id }),
        }
    }
}

impl Read {
    /// The values the current relation is joined on, from the rows of the
    /// relation on the other side of the join's first equality.
    fn key_hint(&self) -> Option<KeyHint> {
        let join = self
            .plan
            .joins
            .iter()
            .find(|join| join.relation == self.current)?;
        let (left, right) = *join.on.first()?;
        let owner = binding(&self.plan.bindings, left)?;
        let mut values: Vec<Cell> = Vec::new();
        for row in &self.fetched[owner.relation] {
            if let Some(cell) = row.cells.get(&left)
                && !values.contains(cell)
            {
                values.push(cell.clone());
            }
        }
        Some(KeyHint {
            column: binding(&self.plan.bindings, right)?.column,
            values: flatten_entities(values),
        })
    }

    fn outcome(&self, rows: crate::fold::Table, row_ids: Vec<Uuid>) -> Outcome {
        Outcome {
            columns: self.columns.clone(),
            rows,
            row_ids,
            read_tables: self
                .plan
                .relations
                .iter()
                .map(|relation| relation.relation.table)
                .collect(),
            truncated: self.truncated,
            ..Outcome::default()
        }
    }
}

/// Start fetching the current relation.
fn begin_relation(read: &mut Read, requests: &mut Requests) -> Step {
    read.fetched.push(Vec::new());
    let query = read.plan.relations[read.current].query.clone();
    if matches!(query, GqlQuery::GroupSoup { .. }) {
        return Step::Bins(Request {
            id: requests.issue(),
            query,
            needs: Vec::new(),
            cursor: None,
            limit: 0,
        });
    }
    request(read, requests, None)
}

/// The next page of the current relation, with the join's key hint filled
/// in.
fn request(read: &Read, requests: &mut Requests, cursor: Option<String>) -> Step {
    let relation = &read.plan.relations[read.current];
    let mut query = relation.query.clone();
    if let Some(hint) = read.key_hint() {
        match &mut query {
            GqlQuery::Soup { key_hint, .. } => *key_hint = Some(hint),
            GqlQuery::People { ids } => {
                *ids = Some(
                    hint.values
                        .into_iter()
                        .flat_map(|cell| match cell {
                            Cell::Entities(ids) => ids,
                            Cell::Text(id) => vec![id],
                            _ => Vec::new(),
                        })
                        .collect(),
                )
            }
            GqlQuery::GroupSoup { .. } => {}
        }
    }
    let fetched = read.fetched[read.current].len();
    Step::Fetch(Request {
        id: requests.issue(),
        query,
        needs: relation.needs.clone(),
        cursor,
        limit: (ROW_CAP - fetched).min(PAGE_LIMIT),
    })
}

/// A joined relation's cells under the keys the plan uses for it: a cell
/// keyed by one of its table's definitions moves to that column's key for
/// this relation. The `FROM` relation's keys are its definitions already,
/// and a cell keyed some other way is kept as is.
fn keyed(catalog: &Catalog, read: &Read, rows: Vec<Row>) -> Vec<Row> {
    if read.current == 0 {
        return rows;
    }
    let table = table(catalog, read.plan.relations[read.current].relation.table);
    rows.into_iter()
        .map(|row| Row {
            id: row.id,
            position: row.position,
            cells: row
                .cells
                .into_iter()
                .map(|(key, cell)| {
                    if table.columns.iter().any(|column| column.id == key) {
                        (column_key(read.current, key), cell)
                    } else {
                        (key, cell)
                    }
                })
                .collect(),
        })
        .collect()
}

/// A table the statement reads or writes.
fn table(catalog: &Catalog, id: Uuid) -> &Table {
    catalog
        .tables
        .iter()
        .find(|table| table.id == id)
        .expect("the engine resolved its statement against this catalog")
}

/// The rows a `WHERE row_id = …` or `WHERE row_id IN (…)` names, which must
/// all exist: naming a row is not a search.
fn named_rows(read: &SelectQuery) -> Vec<Uuid> {
    let row_id = row_id_key(read.table());
    let ids = |values: &[Value]| -> Vec<Uuid> {
        values
            .iter()
            .filter_map(|value| match value {
                Value::Entity(id) => Uuid::parse_str(id).ok(),
                _ => None,
            })
            .collect()
    };
    match &read.where_ {
        Some(Filter::Comparison {
            column,
            operator: ComparisonOperator::Equal,
            value,
        }) if *column == row_id => ids(std::slice::from_ref(value)),
        Some(Filter::In {
            column,
            values,
            negated: false,
        }) if *column == row_id => ids(values),
        _ => Vec::new(),
    }
}

fn reading(read: &mut Option<Read>) -> Result<&mut Read, RunError> {
    read.as_mut().ok_or(RunError::Results {
        message: "rows were fed to a statement that reads none".into(),
    })
}

/// Position, then id, as `database_rows` orders a table; rows without a
/// position (`people`) keep the order they came in, after any that have one.
fn table_order(a: &Row, b: &Row) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (&a.position, &b.position) {
        (Some(left), Some(right)) => left.cmp(right).then(a.id.cmp(&b.id)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Entity cells one id each, so a driver can read the hint as a list of ids.
fn flatten_entities(values: Vec<Cell>) -> Vec<Cell> {
    let mut out: Vec<Cell> = Vec::new();
    for value in values {
        match value {
            Cell::Entities(ids) => {
                for id in ids {
                    let cell = Cell::Entities(vec![id]);
                    if !out.contains(&cell) {
                        out.push(cell);
                    }
                }
            }
            other => {
                if !out.contains(&other) {
                    out.push(other);
                }
            }
        }
    }
    out
}
