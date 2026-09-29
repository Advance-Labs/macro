//! Printing shared by the examples.

use database_sql::catalog::{Catalog, ColumnKind};
use database_sql::fold::Cell;
use database_sql::resolve::{Query, compile};
use database_sql::run::{Outcome, OutcomeKind};
use database_sql::split::{GqlQuery, split};
use uuid::Uuid;

/// A cell as a person would read it: option labels, not ids.
pub fn show(catalog: &Catalog, value: Option<&Cell>) -> String {
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

/// The result as a table, or the write summary.
pub fn print_outcome(catalog: &Catalog, outcome: &Outcome) {
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
        .map(|column| column.name.chars().count())
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

/// What the statement would send to the server and keep for the fold.
pub fn print_plan(catalog: &Catalog, sql: &str) {
    let Ok(Query::Select(select)) = compile(catalog, sql) else {
        return;
    };
    let plan = split(catalog, select);
    let propf = |propf: &Option<_>| {
        propf
            .as_ref()
            .map(|expr| serde_json::to_string(expr).unwrap())
            .unwrap_or_else(|| "none".into())
    };
    match &plan.gql {
        GqlQuery::Soup { propf: p, .. } => println!("  gql: soup, propf = {}", propf(p)),
        GqlQuery::GroupSoup { propf: p, .. } => println!(
            "  gql: groupSoup (bins only, no rows fetched), propf = {}",
            propf(p)
        ),
    }
    println!(
        "  residual: {}",
        plan.residual
            .as_ref()
            .map(|filter| format!("{filter:?}"))
            .unwrap_or_else(|| "none".into())
    );
}
