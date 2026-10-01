//! A view as the SQL statement it compiles to, for people to read. It is
//! printed from the compiled query, so it says what runs.

#[cfg(test)]
mod test;

use chrono::SecondsFormat;
use models_databases::views::{DatabaseView, ViewProblem};
use uuid::Uuid;

use crate::catalog::{Catalog, ColumnKind, Table};
use crate::resolve::{Filter, OrderKey, ROW_POSITION, Value, row_position_key};

use super::checked_table;
use super::compile::compile_checked;

/// The view as a `SELECT` in this crate's dialect; compiling it gives back
/// [`compile_view`](super::compile_view)'s query.
pub fn view_as_sql(view: &DatabaseView, catalog: &Catalog) -> Result<String, ViewProblem> {
    let table = checked_table(view, catalog)?;
    let query = compile_checked(view, table);
    let mut sql = format!(
        "SELECT * FROM {}.{}",
        identifier(&table.database),
        identifier(&table.name)
    );
    if let Some(filter) = &query.where_ {
        sql.push_str(" WHERE ");
        sql.push_str(&condition(table, filter));
    }
    let keys: Vec<String> = query
        .order_by
        .iter()
        .map(|order| {
            let OrderKey::Column(key) = order.key else {
                unreachable!("a view sorts by columns");
            };
            if key == row_position_key(table.id) {
                ROW_POSITION.to_string()
            } else {
                format!(
                    "{} {}",
                    identifier(column_name(table, key)),
                    <&str>::from(order.dir)
                )
            }
        })
        .collect();
    sql.push_str(" ORDER BY ");
    sql.push_str(&keys.join(", "));
    Ok(sql)
}

fn condition(table: &Table, filter: &Filter) -> String {
    let column = |key: &Uuid| identifier(column_name(table, *key));
    let negation = |negated: bool| if negated { "NOT " } else { "" };
    match filter {
        Filter::And(parts) => joined(table, parts, " AND "),
        Filter::Or(parts) => joined(table, parts, " OR "),
        Filter::Cmp {
            column: key,
            op,
            value,
        } => {
            format!(
                "{} {} {}",
                column(key),
                op.symbol(),
                literal(table, *key, value)
            )
        }
        Filter::In {
            column: key,
            values,
            negated,
        } => format!(
            "{} {}IN ({})",
            column(key),
            negation(*negated),
            values
                .iter()
                .map(|value| literal(table, *key, value))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Filter::Has {
            column: key,
            value,
            negated,
        } => format!(
            "{} {}HAS {}",
            column(key),
            negation(*negated),
            literal(table, *key, value)
        ),
        Filter::IsNull {
            column: key,
            negated,
        } => format!("{} IS {}NULL", column(key), negation(*negated)),
        Filter::Like {
            column: key,
            pattern,
            escape,
            negated,
        } => {
            let escape = escape
                .map(|escape| format!(" ESCAPE {}", string(&escape.to_string())))
                .unwrap_or_default();
            format!(
                "{} {}LIKE {}{escape}",
                column(key),
                negation(*negated),
                string(pattern)
            )
        }
    }
}

/// Conditions joined by one keyword, a nested `AND` or `OR` parenthesized
/// so it parses back as the same tree.
fn joined(table: &Table, parts: &[Filter], keyword: &str) -> String {
    parts
        .iter()
        .map(|part| match part {
            Filter::And(_) | Filter::Or(_) => format!("({})", condition(table, part)),
            _ => condition(table, part),
        })
        .collect::<Vec<_>>()
        .join(keyword)
}

fn literal(table: &Table, key: Uuid, value: &Value) -> String {
    match value {
        Value::Text(text) | Value::Entity(text) => string(text),
        Value::Number(number) => number.to_string(),
        Value::Bool(true) => "TRUE".into(),
        Value::Bool(false) => "FALSE".into(),
        Value::Date(date) => string(&date.to_rfc3339_opts(SecondsFormat::AutoSi, true)),
        Value::Option(option) => string(option_label(table, key, *option)),
        Value::Options(_) | Value::Entities(_) => {
            unreachable!("a view compares against one value at a time")
        }
    }
}

fn option_label(table: &Table, key: Uuid, option: Uuid) -> &str {
    table
        .columns
        .iter()
        .find(|column| column.id == key)
        .and_then(|column| match &column.kind {
            ColumnKind::Select { options, .. } => {
                options.iter().find(|candidate| candidate.id == option)
            }
            _ => None,
        })
        .map(|option| option.label.as_str())
        .expect("the view check found the option on the column")
}

fn column_name(table: &Table, key: Uuid) -> &str {
    table
        .columns
        .iter()
        .find(|column| column.id == key)
        .map(|column| column.name.as_str())
        .expect("a view's keys are its table's columns")
}

fn identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn string(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}
