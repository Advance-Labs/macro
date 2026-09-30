//! SQL over a viewer's tables: the `database_sql` engine, fed by the row
//! and cell stores and writing back through them.
//!
//! The engine pushes what Soup could evaluate into a `propf` expression;
//! here, with rows loaded straight from Postgres, that expression is applied
//! in memory with Soup's semantics, so the plan is the same one the browser
//! will run and the fold sees exactly what it would see there.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use database_sql::fold::{Bin, Cell, Row as EngineRow};
use database_sql::resolve::{Query, Value, column_key};
use database_sql::run::{
    Outcome, OutcomeKind, Page, RowSource, RowWriter, RunError, SourceError, WriteError, run,
};
use database_sql::split::GqlQuery;
use filter_ast::Expr;
use item_filters::ast::properties::{PropertiesLiteral, PropertyMatchValue};
use macro_event_broker::MacroEventBroker;
use models_permissions::share_permission::access_level::AccessLevel;
use models_properties::service::property_value::PropertyValue;
use models_properties::shared::EntityReference;
use uuid::Uuid;

use super::{DatabasesServiceImpl, MAX_SQL_LEN, events, infra};
use crate::domain::catalog::{self, ColumnEntry, TableEntry};
use crate::domain::models::{
    DatabaseId, ExecOutcome, ExecRequest, PropertyDefinitionId, QueryError, QueryResult,
    ResultColumn, RowId, SqlValue, TableId, TableVersion, Viewer,
};
use crate::domain::ports::{
    AccessDirectory, CellStore, ColumnDefinitionStore, DatabasesRepo, TableEventPublisher,
};

/// The most joined relations a statement can have; keys are derived per
/// relation index, and the source must recognise every one it may be asked
/// for.
const MAX_RELATIONS: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum QueryMode {
    ReadOnly,
    ReadWrite,
}

impl<Repo, Defs, Cells, Events, Access, Broker>
    DatabasesServiceImpl<Repo, Defs, Cells, Events, Access, Broker>
where
    Repo: DatabasesRepo,
    Defs: ColumnDefinitionStore,
    Cells: CellStore,
    Events: TableEventPublisher,
    Access: AccessDirectory,
    Broker: MacroEventBroker,
{
    pub(super) async fn run_sql(
        &self,
        viewer: Viewer,
        req: ExecRequest,
        mode: QueryMode,
    ) -> Result<ExecOutcome, QueryError> {
        if req.sql.len() > MAX_SQL_LEN {
            return Err(QueryError::BudgetExceeded);
        }
        let entries = self.viewer_entries(&viewer, req.scope).await?;
        let engine_catalog = catalog::engine_catalog(&entries);

        let query = database_sql::compile(&engine_catalog, &req.sql)
            .map_err(|error| QueryError::Sql(error.to_string()))?;
        let written_table = match &query {
            Query::Select(_) => None,
            Query::Insert(insert) => Some(insert.table),
            Query::Update(update) => Some(update.table),
            Query::Delete(delete) => Some(delete.table),
        };
        if let Some(table) = written_table {
            if mode == QueryMode::ReadOnly {
                return Err(QueryError::ReadOnly("queries cannot change data".into()));
            }
            let entry = entry_for(&entries, table)?;
            if entry.grant < AccessLevel::Edit {
                return Err(QueryError::ReadOnly(format!(
                    "table {} is read-only",
                    entry.table.name
                )));
            }
            if let Some(expected) = req
                .base_versions
                .as_ref()
                .and_then(|versions| versions.get(&table))
                && *expected != entry.table.version
            {
                return Err(QueryError::VersionConflict { table_id: table });
            }
        }

        let source = Source {
            service: self,
            entries: &entries,
            loaded: Mutex::new(HashMap::new()),
        };
        let writer = Writer {
            service: self,
            entries: &entries,
            viewer: &viewer,
        };
        let outcome = run(&engine_catalog, &req.sql, &source, &writer)
            .await
            .map_err(|error| match error {
                RunError::Compile(error) => QueryError::Sql(error.to_string()),
                other => QueryError::Sql(other.to_string()),
            })?;
        if let Some(failure) = outcome.failures.first() {
            return Err(QueryError::Sql(format!(
                "row {}: {}",
                failure.row + 1,
                failure.message
            )));
        }

        let mut new_versions = HashMap::new();
        if let Some(table) = written_table
            && outcome.changes_applied > 0
        {
            let version = self.repo.bump_table_version(table).await.map_err(infra)?;
            new_versions.insert(table, version);
        }
        let database_of: HashMap<TableId, DatabaseId> = entries
            .iter()
            .map(|entry| (entry.table.id, entry.table.database_id))
            .collect();
        self.publish(
            Some(events::Attribution::acting(
                viewer.user_id.clone(),
                viewer.acting_bot,
            )),
            &database_of,
            &new_versions,
        )
        .await;

        let read_tables: Vec<TableId> = outcome.read_tables.clone();
        let read_versions: HashMap<TableId, TableVersion> = read_tables
            .iter()
            .filter_map(|table| {
                entries
                    .iter()
                    .find(|entry| entry.table.id == *table)
                    .map(|entry| (*table, entry.table.version))
            })
            .collect();
        let truncated_tables = if outcome.truncated {
            read_tables
                .iter()
                .filter_map(|table| entry_for(&entries, *table).ok())
                .map(|entry| entry.table.name.clone())
                .collect()
        } else {
            Vec::new()
        };
        Ok(ExecOutcome {
            results: result_sets(&entries, &outcome),
            changes_applied: outcome.changes_applied as usize,
            inserted_row_ids: outcome.inserted_row_ids.clone(),
            new_versions,
            read_database_ids: read_tables
                .iter()
                .filter_map(|table| database_of.get(table).copied())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect(),
            read_tables,
            read_versions,
            truncated_tables,
        })
    }

    /// Every table the viewer can see, `scope`'s winning name ties.
    pub(super) async fn viewer_entries(
        &self,
        viewer: &Viewer,
        scope: Option<DatabaseId>,
    ) -> Result<Vec<TableEntry>, QueryError> {
        let grants: HashMap<DatabaseId, _> = self
            .access
            .accessible_databases(viewer)
            .await
            .map_err(infra)?
            .into_iter()
            .collect();
        let mut entries = self.entries_for(&grants).await?;
        if let Some(scope) = scope {
            catalog::scope_entries(&mut entries, scope);
        }
        Ok(entries)
    }

    /// A table's rows with their cells, in position order.
    pub(super) async fn rows_with_cells(
        &self,
        table_id: TableId,
    ) -> Result<Vec<(RowId, HashMap<PropertyDefinitionId, PropertyValue>)>, QueryError> {
        let refs = self.repo.row_refs(table_id).await.map_err(infra)?;
        let ids: Vec<RowId> = refs.iter().map(|row| row.id).collect();
        let mut cells = self.cells.cells(&ids).await.map_err(infra)?;
        Ok(refs
            .into_iter()
            .map(|row| (row.id, cells.remove(&row.id).unwrap_or_default()))
            .collect())
    }
}

fn entry_for(entries: &[TableEntry], table: TableId) -> Result<&TableEntry, QueryError> {
    entries
        .iter()
        .find(|entry| entry.table.id == table)
        .ok_or_else(|| QueryError::Sql(format!("no such table: {table}")))
}

// ---- reads ---------------------------------------------------------------

/// The engine's row source: whole tables loaded once per statement, the
/// pushed-down filter applied in memory, pages sliced from the result.
struct Source<'a, Service> {
    service: &'a Service,
    entries: &'a [TableEntry],
    /// Per table: its rows keyed by definition id, in position order.
    loaded: Mutex<HashMap<TableId, Vec<EngineRow>>>,
}

impl<Repo, Defs, Cells, Events, Access, Broker>
    Source<'_, DatabasesServiceImpl<Repo, Defs, Cells, Events, Access, Broker>>
where
    Repo: DatabasesRepo,
    Defs: ColumnDefinitionStore,
    Cells: CellStore,
    Events: TableEventPublisher,
    Access: AccessDirectory,
    Broker: MacroEventBroker,
{
    async fn table_rows(&self, table: TableId) -> Result<Vec<EngineRow>, SourceError> {
        if let Some(rows) = self.loaded.lock().expect("row cache").get(&table) {
            return Ok(rows.clone());
        }
        let entry =
            entry_for(self.entries, table).map_err(|error| SourceError(error.to_string()))?;
        let rows: Vec<EngineRow> = self
            .service
            .rows_with_cells(table)
            .await
            .map_err(|error| SourceError(error.to_string()))?
            .into_iter()
            .map(|(id, cells)| EngineRow {
                id,
                cells: cells
                    .into_iter()
                    .filter_map(|(definition, value)| {
                        let column = entry.column_for(definition)?;
                        Some((definition, cell(column, &value)))
                    })
                    .collect(),
            })
            .collect();
        self.loaded
            .lock()
            .expect("row cache")
            .insert(table, rows.clone());
        Ok(rows)
    }

    /// The rows the query's `propf` admits, with cells keyed the way the
    /// engine asked for them.
    async fn matching(
        &self,
        table: TableId,
        propf: &Option<Expr<PropertiesLiteral>>,
        needs: &[Uuid],
    ) -> Result<Vec<EngineRow>, SourceError> {
        let rows = self.table_rows(table).await?;
        Ok(rows
            .into_iter()
            .filter(|row| propf.as_ref().is_none_or(|expr| soup_matches(expr, row)))
            .map(|row| rekey(row, needs))
            .collect())
    }
}

impl<Repo, Defs, Cells, Events, Access, Broker> RowSource
    for Source<'_, DatabasesServiceImpl<Repo, Defs, Cells, Events, Access, Broker>>
where
    Repo: DatabasesRepo,
    Defs: ColumnDefinitionStore,
    Cells: CellStore,
    Events: TableEventPublisher,
    Access: AccessDirectory,
    Broker: MacroEventBroker,
{
    async fn page(
        &self,
        query: &GqlQuery,
        needs: &[Uuid],
        cursor: Option<String>,
        limit: usize,
    ) -> Result<Page, SourceError> {
        let GqlQuery::Soup { table, propf, .. } = query else {
            return Err(SourceError("only database tables can be read here".into()));
        };
        let rows = self.matching(*table, propf, needs).await?;
        let start: usize = cursor
            .map(|cursor| cursor.parse())
            .transpose()
            .map_err(|_| SourceError("bad cursor".into()))?
            .unwrap_or(0);
        let end = start.saturating_add(limit).min(rows.len());
        Ok(Page {
            rows: rows[start.min(end)..end].to_vec(),
            next: (end < rows.len()).then(|| end.to_string()),
        })
    }

    async fn bins(&self, query: &GqlQuery) -> Result<Vec<Bin>, SourceError> {
        let GqlQuery::GroupSoup {
            table,
            propf,
            group_by,
            ..
        } = query
        else {
            return Err(SourceError("bins need a grouped query".into()));
        };
        let rows = self.matching(*table, propf, &[*group_by]).await?;
        let mut bins: Vec<Bin> = Vec::new();
        for row in rows {
            let key = row
                .cells
                .get(group_by)
                .cloned()
                .filter(|cell| !cell_is_empty(cell));
            match bins.iter_mut().find(|bin| bin.key == key) {
                Some(bin) => bin.count += 1,
                None => bins.push(Bin { key, count: 1 }),
            }
        }
        Ok(bins)
    }
}

/// Re-key a row's definition-keyed cells to the keys the engine uses for the
/// relation it is fetching: the definition id itself for the `FROM` table,
/// a derived key for a joined one.
fn rekey(row: EngineRow, needs: &[Uuid]) -> EngineRow {
    let mut cells = HashMap::with_capacity(row.cells.len());
    for (definition, cell) in row.cells {
        for relation in 0..MAX_RELATIONS {
            let key = column_key(relation, definition);
            if relation == 0 || needs.contains(&key) {
                cells.insert(key, cell.clone());
            }
        }
    }
    EngineRow { id: row.id, cells }
}

/// Soup's evaluation of a `propf` expression: option or reference membership,
/// `not` as a set difference (so an empty cell passes it).
fn soup_matches(expr: &Expr<PropertiesLiteral>, row: &EngineRow) -> bool {
    match expr {
        Expr::And(a, b) => soup_matches(a, row) && soup_matches(b, row),
        Expr::Or(a, b) => soup_matches(a, row) || soup_matches(b, row),
        Expr::Not(a) => !soup_matches(a, row),
        Expr::Literal(literal) => match (
            row.cells.get(&literal.property_definition_id),
            &literal.value,
        ) {
            (Some(Cell::Options(ids)), PropertyMatchValue::SelectOption(id)) => ids.contains(id),
            (Some(Cell::Entities(ids)), PropertyMatchValue::EntityRef(id)) => {
                let id = id.to_string();
                ids.contains(&id)
            }
            _ => false,
        },
    }
}

fn cell_is_empty(cell: &Cell) -> bool {
    matches!(cell, Cell::Options(ids) if ids.is_empty())
        || matches!(cell, Cell::Entities(ids) if ids.is_empty())
}

/// A stored value as the engine sees it.
fn cell(column: &ColumnEntry, value: &PropertyValue) -> Cell {
    let _ = column;
    match value {
        PropertyValue::Bool(b) => Cell::Bool(*b),
        PropertyValue::Num(n) => Cell::Number(*n),
        PropertyValue::Str(s) => Cell::Text(s.clone()),
        PropertyValue::Date(d) => Cell::Date(*d),
        PropertyValue::SelectOption(ids) => Cell::Options(ids.clone()),
        PropertyValue::EntityRef(refs) => {
            Cell::Entities(refs.iter().map(|r| r.entity_id.clone()).collect())
        }
        PropertyValue::Link(urls) => Cell::Text(urls.join(" ")),
    }
}

// ---- writes --------------------------------------------------------------

/// The engine's writer: row identities through the repository, cells through
/// the cell store, one row per call.
struct Writer<'a, Service> {
    service: &'a Service,
    entries: &'a [TableEntry],
    viewer: &'a Viewer,
}

impl<Repo, Defs, Cells, Events, Access, Broker>
    Writer<'_, DatabasesServiceImpl<Repo, Defs, Cells, Events, Access, Broker>>
where
    Repo: DatabasesRepo,
    Defs: ColumnDefinitionStore,
    Cells: CellStore,
    Events: TableEventPublisher,
    Access: AccessDirectory,
    Broker: MacroEventBroker,
{
    fn entry(&self, table: TableId) -> Result<&TableEntry, WriteError> {
        let entry =
            entry_for(self.entries, table).map_err(|error| WriteError(error.to_string()))?;
        if entry.grant < AccessLevel::Edit {
            return Err(WriteError(format!(
                "table {} is read-only",
                entry.table.name
            )));
        }
        Ok(entry)
    }

    /// Check the row belongs to the table the statement named, so a row id
    /// from a table the viewer cannot write is refused.
    async fn own_row(&self, table: TableId, row: RowId) -> Result<(), WriteError> {
        match self
            .service
            .repo
            .row_table(row)
            .await
            .map_err(|error| WriteError(error.to_string()))?
        {
            Some(owner) if owner == table => Ok(()),
            _ => Err(WriteError(format!("no row {row} in this table"))),
        }
    }

    /// A first value settles the columns it landed in: they no longer infer
    /// their type from it.
    async fn settle(
        &self,
        table: TableId,
        stored: &[(PropertyDefinitionId, Option<PropertyValue>)],
    ) -> Result<(), WriteError> {
        let valued: Vec<PropertyDefinitionId> = stored
            .iter()
            .filter(|(_, value)| value.is_some())
            .map(|(definition, _)| *definition)
            .collect();
        self.service
            .repo
            .settle_inference(table, &valued)
            .await
            .map_err(|error| WriteError(error.to_string()))
    }

    fn stored(
        entry: &TableEntry,
        cells: Vec<(Uuid, Option<Value>)>,
    ) -> Result<Vec<(PropertyDefinitionId, Option<PropertyValue>)>, WriteError> {
        cells
            .into_iter()
            .map(|(definition, value)| {
                let column = entry
                    .column_for(definition)
                    .ok_or_else(|| WriteError(format!("no column {definition}")))?;
                Ok((definition, value.map(|value| property_value(column, value))))
            })
            .collect()
    }
}

impl<Repo, Defs, Cells, Events, Access, Broker> RowWriter
    for Writer<'_, DatabasesServiceImpl<Repo, Defs, Cells, Events, Access, Broker>>
where
    Repo: DatabasesRepo,
    Defs: ColumnDefinitionStore,
    Cells: CellStore,
    Events: TableEventPublisher,
    Access: AccessDirectory,
    Broker: MacroEventBroker,
{
    async fn insert(&self, table: TableId, cells: Vec<(Uuid, Value)>) -> Result<Uuid, WriteError> {
        let entry = self.entry(table)?;
        let stored = Self::stored(
            entry,
            cells
                .into_iter()
                .map(|(definition, value)| (definition, Some(value)))
                .collect(),
        )?;
        let rows = self
            .service
            .repo
            .insert_rows(table, self.viewer.user_id.as_ref(), 1)
            .await
            .map_err(|error| WriteError(error.to_string()))?
            .ok_or_else(|| WriteError("the table is gone".into()))?;
        let row = rows
            .into_iter()
            .next()
            .ok_or_else(|| WriteError("no row was created".into()))?;
        if !stored.is_empty() {
            self.service
                .cells
                .write(row.id, &stored)
                .await
                .map_err(|error| WriteError(error.to_string()))?;
            self.settle(table, &stored).await?;
        }
        Ok(row.id)
    }

    async fn update(
        &self,
        table: TableId,
        row_id: Uuid,
        cells: Vec<(Uuid, Option<Value>)>,
    ) -> Result<(), WriteError> {
        let entry = self.entry(table)?;
        self.own_row(table, row_id).await?;
        let stored = Self::stored(entry, cells)?;
        self.service
            .cells
            .write(row_id, &stored)
            .await
            .map_err(|error| WriteError(error.to_string()))?;
        self.settle(table, &stored).await
    }

    async fn delete(&self, table: TableId, row_id: Uuid) -> Result<(), WriteError> {
        self.entry(table)?;
        self.own_row(table, row_id).await?;
        self.service
            .cells
            .clear(row_id)
            .await
            .map_err(|error| WriteError(error.to_string()))?;
        let removed = self
            .service
            .repo
            .delete_row(table, row_id)
            .await
            .map_err(|error| WriteError(error.to_string()))?;
        if !removed {
            return Err(WriteError(format!("no row {row_id} in this table")));
        }
        Ok(())
    }
}

/// A typed value as the properties system stores it. Entity references take
/// the column's target type; a relation column targets rows.
fn property_value(column: &ColumnEntry, value: Value) -> PropertyValue {
    let entity_type = if column.is_relation() {
        models_properties::EntityType::DatabaseRow
    } else {
        column
            .definition
            .definition
            .specific_entity_type
            .unwrap_or(models_properties::EntityType::User)
    };
    let reference = |id: String| EntityReference {
        entity_id: id,
        entity_type,
        specific_message_id: None,
    };
    match value {
        Value::Text(text) => {
            if column.definition.definition.data_type == models_properties::shared::DataType::Link {
                PropertyValue::Link(vec![text])
            } else {
                PropertyValue::Str(text)
            }
        }
        Value::Number(n) => PropertyValue::Num(n),
        Value::Bool(b) => PropertyValue::Bool(b),
        Value::Date(d) => PropertyValue::Date(d),
        Value::Option(id) => PropertyValue::SelectOption(vec![id]),
        Value::Options(ids) => PropertyValue::SelectOption(ids),
        Value::Entity(id) => PropertyValue::EntityRef(vec![reference(id)]),
        Value::Entities(ids) => PropertyValue::EntityRef(ids.into_iter().map(reference).collect()),
    }
}

// ---- the response shape the clients know ----------------------------------

/// The engine's outcome as the exec API's result sets: `row_id` first for
/// row-shaped results, values as the scalars clients already decode.
fn result_sets(entries: &[TableEntry], outcome: &Outcome) -> Vec<QueryResult> {
    if outcome.columns.is_empty() {
        return Vec::new();
    }
    let row_shaped = outcome.row_ids.len() == outcome.rows.len() && !outcome.rows.is_empty()
        || (outcome.rows.is_empty() && outcome.columns.iter().all(|c| c.column.is_some()));
    let column_entry = |definition: Option<Uuid>| {
        definition.and_then(|definition| {
            entries
                .iter()
                .find_map(|entry| entry.column_for(definition).map(|column| (entry, column)))
        })
    };
    let mut columns: Vec<ResultColumn> = Vec::new();
    if row_shaped {
        columns.push(ResultColumn {
            name: "row_id".into(),
            entity_type: None,
            origin: None,
        });
    }
    columns.extend(outcome.columns.iter().map(|column| {
        let found = column_entry(column.column);
        ResultColumn {
            name: column.name.clone(),
            entity_type: match (column.kind, found) {
                (OutcomeKind::Entity, Some((_, entry))) if entry.is_relation() => {
                    Some(model_entity::EntityType::DatabaseRow)
                }
                (OutcomeKind::Entity, Some((_, entry))) => entry
                    .definition
                    .definition
                    .specific_entity_type
                    .map(entity_type_for),
                _ => None,
            },
            origin: found.map(|(table, entry)| (table.table.name.clone(), entry.name().to_owned())),
        }
    }));
    let rows = outcome
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let mut values = Vec::with_capacity(row.len() + 1);
            if row_shaped {
                values.push(SqlValue::Text(outcome.row_ids[index].to_string()));
            }
            values.extend(row.iter().zip(&outcome.columns).map(|(cell, column)| {
                sql_value(
                    cell.as_ref(),
                    column_entry(column.column).map(|(_, entry)| entry),
                )
            }));
            values
        })
        .collect();
    vec![QueryResult { columns, rows }]
}

/// A result cell as the scalar the clients decode: labels for options, ids
/// for references, JSON arrays for multi-valued cells.
fn sql_value(cell: Option<&Cell>, column: Option<&ColumnEntry>) -> SqlValue {
    let Some(cell) = cell else {
        return SqlValue::Null;
    };
    let multi = column.is_some_and(ColumnEntry::is_multi);
    match cell {
        Cell::Text(text) => SqlValue::Text(text.clone()),
        Cell::Number(n) => SqlValue::Real(*n),
        Cell::Bool(b) => SqlValue::Integer(i64::from(*b)),
        Cell::Date(d) => SqlValue::Text(d.to_rfc3339()),
        Cell::Options(ids) => {
            let labels: Vec<String> = ids
                .iter()
                .map(|id| {
                    column
                        .and_then(|column| {
                            catalog::option_labels(&column.definition)
                                .into_iter()
                                .find(|(candidate, _)| candidate == id)
                                .map(|(_, label)| label)
                        })
                        .unwrap_or_else(|| id.to_string())
                })
                .collect();
            scalar_or_array(labels, multi)
        }
        Cell::Entities(ids) => scalar_or_array(ids.clone(), multi),
    }
}

fn scalar_or_array(values: Vec<String>, multi: bool) -> SqlValue {
    if multi {
        SqlValue::Text(
            serde_json::Value::Array(values.into_iter().map(Into::into).collect()).to_string(),
        )
    } else {
        values
            .into_iter()
            .next()
            .map_or(SqlValue::Null, SqlValue::Text)
    }
}

/// The platform entity type an entity column's references point at.
fn entity_type_for(property_type: models_properties::EntityType) -> model_entity::EntityType {
    use model_entity::EntityType as E;
    use models_properties::EntityType as P;
    match property_type {
        P::User => E::User,
        P::Document | P::Task => E::Document,
        P::Company => E::CrmCompany,
        P::CallRecord => E::Call,
        P::Channel => E::Channel,
        P::Chat => E::Chat,
        P::Project => E::Project,
        P::Thread => E::EmailThread,
        P::CalendarEvent => E::CalendarEvent,
        P::Initiative => E::Initiative,
        P::DatabaseRow => E::DatabaseRow,
    }
}
