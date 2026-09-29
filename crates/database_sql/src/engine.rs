//! Stage four as a state machine: an [`Engine`] carries one `SELECT` from
//! its first fetch to its [`Outcome`], asking the driver for rows one
//! [`Request`] at a time and taking them back through [`Engine::feed_page`]
//! and [`Engine::feed_bins`].
//!
//! The engine does no I/O and holds no references, so it crosses a wasm
//! boundary as JSON: a driver in any language loops on [`Step`] until it is
//! [`Step::Done`]. Relations are fetched in plan order, each to completion,
//! and a joined relation's request carries the key values the rows so far
//! need (see [`KeyHint`]), which a driver may use to fetch less.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::catalog::Catalog;
use crate::fold::{Bin, Cell, Row, fold_bins, fold_relations};
use crate::resolve::{Query, SelectQuery, compile, row_id_key};
use crate::run::{Outcome, OutcomeColumn, PAGE_LIMIT, Page, ROW_CAP, RunError, describe};
use crate::split::{GqlQuery, KeyHint, Plan, Shape, split};

/// What the driver does next.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "camelCase")]
pub enum Step {
    /// Fetch one page and feed it to [`Engine::feed_page`].
    Fetch(Request),
    /// Fetch the bins of a `GroupSoup` query and feed them to
    /// [`Engine::feed_bins`].
    Bins(Request),
    /// The answer.
    Done(Outcome),
}

/// One fetch the engine wants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub limit: usize,
}

/// One `SELECT` in flight.
#[derive(Debug, Clone)]
pub struct Engine {
    catalog: Catalog,
    columns: Vec<OutcomeColumn>,
    plan: Plan,
    /// The rows of every relation fetched so far, `FROM` first.
    fetched: Vec<Vec<Row>>,
    /// The relation being fetched.
    current: usize,
    truncated: bool,
    outstanding: Option<u32>,
    next_id: u32,
}

impl Engine {
    /// Compile a `SELECT` and ask for its first fetch.
    pub fn start(catalog: &Catalog, sql: &str) -> Result<(Engine, Step), RunError> {
        match compile(catalog, sql)? {
            Query::Select(select) => Ok(Engine::from_select(catalog, select)),
            Query::Insert(_) | Query::Update(_) | Query::Delete(_) => Err(RunError::NotARead),
        }
    }

    /// Plan a resolved `SELECT` and ask for its first fetch.
    pub fn from_select(catalog: &Catalog, select: SelectQuery) -> (Engine, Step) {
        let columns = describe(catalog, &select.items, &select.bindings, &select.relations);
        let plan = split(catalog, select);
        let mut engine = Engine {
            catalog: catalog.clone(),
            columns,
            plan,
            fetched: Vec::new(),
            current: 0,
            truncated: false,
            outstanding: None,
            next_id: 0,
        };
        let step = engine.begin_relation();
        (engine, step)
    }

    /// The plan being run.
    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    /// Take one page of the outstanding request.
    pub fn feed_page(&mut self, request_id: u32, page: Page) -> Result<Step, RunError> {
        self.take(request_id)?;
        let rows = &mut self.fetched[self.current];
        rows.extend(page.rows);
        if rows.len() >= ROW_CAP {
            if rows.len() > ROW_CAP {
                rows.truncate(ROW_CAP);
            }
            self.truncated = true;
            return Ok(self.finish_relation());
        }
        match page.next {
            Some(cursor) => Ok(self.request(Some(cursor))),
            None => Ok(self.finish_relation()),
        }
    }

    /// Take the bins of the outstanding request.
    pub fn feed_bins(&mut self, request_id: u32, bins: Vec<Bin>) -> Result<Step, RunError> {
        self.take(request_id)?;
        let rows = fold_bins(&self.catalog, &self.plan, bins);
        Ok(Step::Done(self.outcome(rows, Vec::new())))
    }

    fn take(&mut self, request_id: u32) -> Result<(), RunError> {
        match self.outstanding.take() {
            Some(expected) if expected == request_id => Ok(()),
            Some(expected) => Err(RunError::WrongRequest {
                expected,
                fed: request_id,
            }),
            None => Err(RunError::NothingOutstanding { fed: request_id }),
        }
    }

    /// Start fetching `self.current`.
    fn begin_relation(&mut self) -> Step {
        self.fetched.push(Vec::new());
        if matches!(
            self.plan.relations[self.current].gql,
            GqlQuery::GroupSoup { .. }
        ) {
            let id = self.next();
            return Step::Bins(Request {
                id,
                query: self.plan.relations[self.current].gql.clone(),
                needs: Vec::new(),
                cursor: None,
                limit: 0,
            });
        }
        self.request(None)
    }

    /// The next page of `self.current`, with the join's key hint filled in.
    fn request(&mut self, cursor: Option<String>) -> Step {
        let relation = &self.plan.relations[self.current];
        let needs = relation.needs.clone();
        let mut query = relation.gql.clone();
        if let Some(hint) = self.key_hint() {
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
        let fetched = self.fetched[self.current].len();
        let id = self.next();
        Step::Fetch(Request {
            id,
            query,
            needs,
            cursor,
            limit: (ROW_CAP - fetched).min(PAGE_LIMIT),
        })
    }

    /// The values the current relation is joined on, from the rows of the
    /// relation on the other side of the join's first equality.
    fn key_hint(&self) -> Option<KeyHint> {
        let join = self
            .plan
            .joins
            .iter()
            .find(|join| join.relation == self.current)?;
        let (left, right) = *join.on.first()?;
        let owner = self
            .plan
            .bindings
            .iter()
            .find(|binding| binding.key == left)?;
        let mut values: Vec<Cell> = Vec::new();
        for row in &self.fetched[owner.relation] {
            if let Some(cell) = row.cells.get(&left) {
                let cell = match cell {
                    Cell::Entities(ids) => Cell::Entities(ids.clone()),
                    other => other.clone(),
                };
                if !values.contains(&cell) {
                    values.push(cell);
                }
            }
        }
        Some(KeyHint {
            column: self
                .plan
                .bindings
                .iter()
                .find(|binding| binding.key == right)?
                .column,
            values: flatten_entities(values),
        })
    }

    /// A relation is complete: give it its row ids, then move on or fold.
    fn finish_relation(&mut self) -> Step {
        let table = self.plan.relations[self.current].relation.table;
        let key = row_id_key(table);
        if self.plan.bindings.iter().any(|binding| binding.key == key) {
            for row in &mut self.fetched[self.current] {
                row.cells
                    .insert(key, Cell::Entities(vec![row.id.to_string()]));
            }
        }
        self.current += 1;
        if self.current < self.plan.relations.len() {
            return self.begin_relation();
        }
        let fetched = std::mem::take(&mut self.fetched);
        let (rows, row_ids) = fold_relations(&self.catalog, &self.plan, fetched);
        let row_ids = match self.plan.shape {
            Shape::Rows(_) => row_ids,
            Shape::Aggregate { .. } => Vec::new(),
        };
        Step::Done(self.outcome(rows, row_ids))
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

    fn next(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.outstanding = Some(id);
        id
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
