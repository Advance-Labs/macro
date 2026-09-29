//! An interactive tour of the engine over an in-memory table.
//!
//! ```sh
//! cargo run -p database_sql --example repl
//! ```
//!
//! Type SQL; see the plan (what would go to the server as a Soup `propf`
//! filter, what the fold keeps), the result, and the errors an agent gets.
//! `INSERT`, `UPDATE … WHERE row_id = …` and `DELETE … WHERE row_id = …`
//! change the in-memory rows. `\rows` dumps them; `\catalog` shows the schema.

use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::Mutex;

use chrono::{TimeZone, Utc};
use database_sql::catalog::{Catalog, Column, ColumnKind, SelectOption, Table};
use database_sql::fold::{Bin, Cell, Row};
use database_sql::resolve::{Query, Value, compile};
use database_sql::run::{
    Outcome, OutcomeKind, Page, RowSource, RowWriter, SourceError, WriteError, run,
};
use database_sql::split::{GqlQuery, split};
use uuid::Uuid;

const DEALS: Uuid = Uuid::from_u128(0xd0);
const NAME: Uuid = Uuid::from_u128(0x01);
const AMOUNT: Uuid = Uuid::from_u128(0x02);
const STAGE: Uuid = Uuid::from_u128(0x03);
const CLOSED_AT: Uuid = Uuid::from_u128(0x04);
const OWNER: Uuid = Uuid::from_u128(0x05);
const TAGS: Uuid = Uuid::from_u128(0x06);
const DONE: Uuid = Uuid::from_u128(0x07);
const LEAD: Uuid = Uuid::from_u128(0x31);
const WON: Uuid = Uuid::from_u128(0x30);
const LOST: Uuid = Uuid::from_u128(0x33);
const VIP: Uuid = Uuid::from_u128(0x32);
const RENEWAL: Uuid = Uuid::from_u128(0x34);

fn catalog() -> Catalog {
    Catalog {
        tables: vec![Table {
            id: DEALS,
            database: "crm".into(),
            name: "deals".into(),
            columns: vec![
                Column {
                    id: NAME,
                    name: "name".into(),
                    kind: ColumnKind::Text,
                },
                Column {
                    id: AMOUNT,
                    name: "amount".into(),
                    kind: ColumnKind::Number,
                },
                Column {
                    id: STAGE,
                    name: "stage".into(),
                    kind: ColumnKind::Select {
                        multi: false,
                        options: vec![
                            SelectOption {
                                id: LEAD,
                                label: "Lead".into(),
                            },
                            SelectOption {
                                id: WON,
                                label: "Won".into(),
                            },
                            SelectOption {
                                id: LOST,
                                label: "Lost".into(),
                            },
                        ],
                    },
                },
                Column {
                    id: CLOSED_AT,
                    name: "closed at".into(),
                    kind: ColumnKind::Date,
                },
                Column {
                    id: OWNER,
                    name: "owner".into(),
                    kind: ColumnKind::Entity { multi: false },
                },
                Column {
                    id: TAGS,
                    name: "tags".into(),
                    kind: ColumnKind::Select {
                        multi: true,
                        options: vec![
                            SelectOption {
                                id: VIP,
                                label: "vip".into(),
                            },
                            SelectOption {
                                id: RENEWAL,
                                label: "renewal".into(),
                            },
                        ],
                    },
                },
                Column {
                    id: DONE,
                    name: "done".into(),
                    kind: ColumnKind::Boolean,
                },
            ],
        }],
    }
}

fn seed() -> Vec<Row> {
    let row = |id: u128, cells: Vec<(Uuid, Cell)>| Row {
        id: Uuid::from_u128(id),
        cells: cells.into_iter().collect(),
    };
    let date = |y, m, d| Cell::Date(Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap());
    vec![
        row(
            0xa1,
            vec![
                (NAME, Cell::Text("Acme".into())),
                (AMOUNT, Cell::Number(12000.0)),
                (STAGE, Cell::Options(vec![WON])),
                (CLOSED_AT, date(2026, 9, 1)),
                (OWNER, Cell::Entities(vec!["macro|sam@example.com".into()])),
                (TAGS, Cell::Options(vec![VIP])),
                (DONE, Cell::Bool(true)),
            ],
        ),
        row(
            0xa2,
            vec![
                (NAME, Cell::Text("Globex".into())),
                (AMOUNT, Cell::Number(3000.0)),
                (STAGE, Cell::Options(vec![LEAD])),
                (OWNER, Cell::Entities(vec!["macro|sam@example.com".into()])),
            ],
        ),
        row(
            0xa3,
            vec![
                (NAME, Cell::Text("Hooli".into())),
                (STAGE, Cell::Options(vec![WON])),
                (CLOSED_AT, date(2026, 9, 15)),
                (OWNER, Cell::Entities(vec!["macro|ana@example.com".into()])),
                (TAGS, Cell::Options(vec![VIP, RENEWAL])),
            ],
        ),
        row(
            0xa4,
            vec![
                (NAME, Cell::Text("Initech".into())),
                (AMOUNT, Cell::Number(7000.0)),
                (STAGE, Cell::Options(vec![LOST])),
                (DONE, Cell::Bool(false)),
            ],
        ),
        row(
            0xa5,
            vec![
                (NAME, Cell::Text("Umbrella".into())),
                (AMOUNT, Cell::Number(45000.0)),
                (OWNER, Cell::Entities(vec!["macro|ana@example.com".into()])),
            ],
        ),
    ]
}

/// The whole "server": rows in memory. Reads honour the pushed-down filter
/// the way Soup would; writes land as cells.
struct Memory {
    rows: Mutex<Vec<Row>>,
}

impl Memory {
    /// Soup semantics for `propf`: `so` and `er` literals test membership,
    /// `not` is a set difference, so empty cells pass a `not`.
    fn matches(
        expr: &filter_ast::Expr<item_filters::ast::properties::PropertiesLiteral>,
        row: &Row,
    ) -> bool {
        use filter_ast::Expr;
        use item_filters::ast::properties::PropertyMatchValue;
        match expr {
            Expr::And(a, b) => Self::matches(a, row) && Self::matches(b, row),
            Expr::Or(a, b) => Self::matches(a, row) || Self::matches(b, row),
            Expr::Not(a) => !Self::matches(a, row),
            Expr::Literal(literal) => match (
                row.cells.get(&literal.property_definition_id),
                &literal.value,
            ) {
                (Some(Cell::Options(ids)), PropertyMatchValue::SelectOption(id)) => {
                    ids.contains(id)
                }
                (Some(Cell::Entities(ids)), PropertyMatchValue::EntityRef(id)) => {
                    ids.iter().any(|candidate| candidate == &id.to_string())
                }
                _ => false,
            },
        }
    }

    fn select(&self, query: &GqlQuery) -> Vec<Row> {
        let propf = match query {
            GqlQuery::Soup { propf, .. } | GqlQuery::GroupSoup { propf, .. } => propf,
        };
        self.rows
            .lock()
            .unwrap()
            .iter()
            .filter(|row| propf.as_ref().is_none_or(|expr| Self::matches(expr, row)))
            .cloned()
            .collect()
    }
}

impl RowSource for Memory {
    async fn page(
        &self,
        query: &GqlQuery,
        _needs: &[Uuid],
        cursor: Option<String>,
        limit: usize,
    ) -> Result<Page, SourceError> {
        let rows = self.select(query);
        let start: usize = cursor.map(|c| c.parse().unwrap_or(0)).unwrap_or(0);
        let end = (start + limit).min(rows.len());
        Ok(Page {
            rows: rows[start..end].to_vec(),
            next: (end < rows.len()).then(|| end.to_string()),
        })
    }

    async fn bins(&self, query: &GqlQuery) -> Result<Vec<Bin>, SourceError> {
        let GqlQuery::GroupSoup { group_by, .. } = query else {
            return Err(SourceError("bins need a groupSoup query".into()));
        };
        let mut bins: Vec<Bin> = Vec::new();
        for row in self.select(query) {
            let key = row.cells.get(group_by).cloned();
            match bins.iter_mut().find(|bin| bin.key == key) {
                Some(bin) => bin.count += 1,
                None => bins.push(Bin { key, count: 1 }),
            }
        }
        Ok(bins)
    }
}

fn cell(value: Value) -> Cell {
    match value {
        Value::Text(text) => Cell::Text(text),
        Value::Number(n) => Cell::Number(n),
        Value::Bool(b) => Cell::Bool(b),
        Value::Date(d) => Cell::Date(d),
        Value::Option(id) => Cell::Options(vec![id]),
        Value::Entity(id) => Cell::Entities(vec![id]),
    }
}

impl RowWriter for Memory {
    async fn insert(&self, _table: Uuid, cells: Vec<(Uuid, Value)>) -> Result<Uuid, WriteError> {
        let id = Uuid::now_v7();
        self.rows.lock().unwrap().push(Row {
            id,
            cells: cells
                .into_iter()
                .map(|(column, value)| (column, cell(value)))
                .collect(),
        });
        Ok(id)
    }

    async fn update(
        &self,
        _table: Uuid,
        row_id: Uuid,
        cells: Vec<(Uuid, Option<Value>)>,
    ) -> Result<(), WriteError> {
        let mut rows = self.rows.lock().unwrap();
        let row = rows
            .iter_mut()
            .find(|row| row.id == row_id)
            .ok_or_else(|| WriteError(format!("no row {row_id}")))?;
        for (column, value) in cells {
            match value {
                Some(value) => {
                    row.cells.insert(column, cell(value));
                }
                None => {
                    row.cells.remove(&column);
                }
            }
        }
        Ok(())
    }

    async fn delete(&self, _table: Uuid, row_id: Uuid) -> Result<(), WriteError> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|row| row.id != row_id);
        if rows.len() == before {
            return Err(WriteError(format!("no row {row_id}")));
        }
        Ok(())
    }
}

fn show(catalog: &Catalog, value: Option<&Cell>) -> String {
    let label = |id: &Uuid| {
        catalog
            .tables
            .iter()
            .flat_map(|table| &table.columns)
            .find_map(|column| match &column.kind {
                ColumnKind::Select { options, .. } => options
                    .iter()
                    .find(|option| option.id == *id)
                    .map(|option| option.label.clone()),
                _ => None,
            })
            .unwrap_or_else(|| id.to_string())
    };
    match value {
        None => "—".into(),
        Some(Cell::Text(text)) => text.clone(),
        Some(Cell::Number(n)) => {
            if n.fract() == 0.0 {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            }
        }
        Some(Cell::Bool(b)) => if *b { "☑" } else { "☐" }.into(),
        Some(Cell::Date(d)) => d.format("%Y-%m-%d").to_string(),
        Some(Cell::Options(ids)) => ids.iter().map(label).collect::<Vec<_>>().join(", "),
        Some(Cell::Entities(ids)) => ids.join(", "),
    }
}

fn print_outcome(catalog: &Catalog, outcome: &Outcome) {
    if outcome.columns.is_empty() {
        println!(
            "  {} row(s) changed{}",
            outcome.changes_applied,
            if outcome.inserted_row_ids.is_empty() {
                String::new()
            } else {
                format!(
                    ", inserted {}",
                    outcome
                        .inserted_row_ids
                        .iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        );
        for failure in &outcome.failures {
            println!("  row {}: {}", failure.row + 1, failure.message);
        }
        return;
    }
    let mut widths: Vec<usize> = outcome
        .columns
        .iter()
        .map(|column| column.name.len())
        .collect();
    let rows: Vec<Vec<String>> = outcome
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .enumerate()
                .map(|(i, value)| {
                    let text = show(catalog, value.as_ref());
                    widths[i] = widths[i].max(text.chars().count());
                    text
                })
                .collect()
        })
        .collect();
    let line = |cells: Vec<String>| {
        cells
            .iter()
            .enumerate()
            .map(|(i, text)| format!("{text:<width$}", width = widths[i]))
            .collect::<Vec<_>>()
            .join("  ")
    };
    println!(
        "  {}",
        line(outcome.columns.iter().map(|c| c.name.clone()).collect())
    );
    println!(
        "  {}",
        line(
            outcome
                .columns
                .iter()
                .map(|c| match c.kind {
                    OutcomeKind::Text => "text",
                    OutcomeKind::Number => "number",
                    OutcomeKind::Boolean => "checkbox",
                    OutcomeKind::Date => "date",
                    OutcomeKind::Select => "select",
                    OutcomeKind::Entity => "entity",
                }
                .into())
                .collect()
        )
    );
    for (i, row) in rows.into_iter().enumerate() {
        let id = outcome
            .row_ids
            .get(i)
            .map(|id| format!("   {id}"))
            .unwrap_or_default();
        println!("  {}{id}", line(row));
    }
    println!(
        "  ({} row(s){})",
        outcome.rows.len(),
        if outcome.truncated { ", truncated" } else { "" }
    );
}

fn print_plan(catalog: &Catalog, sql: &str) {
    let Ok(Query::Select(select)) = compile(catalog, sql) else {
        return;
    };
    let plan = split(catalog, select);
    match &plan.gql {
        GqlQuery::Soup { propf, .. } => {
            println!(
                "  gql: soup, propf = {}",
                propf
                    .as_ref()
                    .map(|expr| serde_json::to_string(expr).unwrap())
                    .unwrap_or_else(|| "none".into())
            );
        }
        GqlQuery::GroupSoup { propf, .. } => {
            println!(
                "  gql: groupSoup (bins only, no rows fetched), propf = {}",
                propf
                    .as_ref()
                    .map(|expr| serde_json::to_string(expr).unwrap())
                    .unwrap_or_else(|| "none".into())
            );
        }
    }
    println!(
        "  residual: {}",
        plan.residual
            .as_ref()
            .map(|filter| format!("{filter:?}"))
            .unwrap_or_else(|| "none".into())
    );
}

fn main() {
    let catalog = catalog();
    let memory = Memory {
        rows: Mutex::new(seed()),
    };
    println!(
        "database_sql repl — table crm.deals(name, amount, stage, \"closed at\", owner, tags, done)"
    );
    println!(
        "try: SELECT name, amount FROM crm.deals WHERE stage = 'Won' AND amount > 5000 ORDER BY amount DESC"
    );
    println!("     SELECT stage, COUNT(*) FROM crm.deals GROUP BY stage");
    println!(
        "     SELECT owner, SUM(amount) FROM crm.deals WHERE tags HAS 'vip' OR amount > 10000 GROUP BY owner"
    );
    println!("     INSERT INTO crm.deals (name, stage, amount) VALUES ('Vandelay', 'Lead', 900)");
    println!("     \\rows  \\catalog  \\q");
    let stdin = io::stdin();
    loop {
        print!("sql> ");
        io::stdout().flush().unwrap();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap() == 0 {
            break;
        }
        let sql = line.trim();
        match sql {
            "" => continue,
            "\\q" => break,
            "\\catalog" => {
                for table in &catalog.tables {
                    println!("  {}.{}", table.database, table.name);
                    for column in &table.columns {
                        println!("    {:<12} {:?}", column.name, column.kind);
                    }
                }
                continue;
            }
            "\\rows" => {
                for row in memory.rows.lock().unwrap().iter() {
                    let cells: HashMap<&Uuid, &Cell> = row.cells.iter().collect();
                    let mut named: Vec<String> = catalog.tables[0]
                        .columns
                        .iter()
                        .filter_map(|column| {
                            cells.get(&column.id).map(|cell| {
                                format!("{}={}", column.name, show(&catalog, Some(cell)))
                            })
                        })
                        .collect();
                    named.insert(0, row.id.to_string());
                    println!("  {}", named.join("  "));
                }
                continue;
            }
            _ => {}
        }
        print_plan(&catalog, sql);
        match pollster::block_on(run(&catalog, sql, &memory, &memory)) {
            Ok(outcome) => print_outcome(&catalog, &outcome),
            Err(error) => println!("  error: {error}"),
        }
    }
}
