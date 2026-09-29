//! Looking names up in the catalog, case-insensitively, with a suggestion
//! when nothing matches.

use crate::catalog::{Catalog, Column, Table};
use crate::parse::{Ident, TableName};

use super::ResolveError;

/// Case-insensitive equality on names.
fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// The catalog table a statement names.
pub fn table<'c>(catalog: &'c Catalog, name: &TableName) -> Result<&'c Table, ResolveError> {
    let matches: Vec<&Table> = catalog
        .tables
        .iter()
        .filter(|table| {
            same(&table.name, &name.table.0)
                && name
                    .database
                    .as_ref()
                    .is_none_or(|database| same(&table.database, &database.0))
        })
        .collect();
    match matches.as_slice() {
        [table] => Ok(table),
        [] => Err(ResolveError::UnknownTable {
            name: written(name),
            suggestion: closest(
                &name.table.0,
                catalog.tables.iter().map(|table| table.name.as_str()),
            )
            .map(|closest| {
                let table = catalog
                    .tables
                    .iter()
                    .find(|table| table.name == closest)
                    .expect("closest name came from the catalog");
                format!("{}.{}", table.database, table.name)
            }),
        }),
        several => Err(ResolveError::AmbiguousTable {
            name: name.table.0.clone(),
            databases: several.iter().map(|table| table.database.clone()).collect(),
        }),
    }
}

/// The table name as the statement wrote it.
fn written(name: &TableName) -> String {
    match &name.database {
        Some(database) => format!("{}.{}", database.0, name.table.0),
        None => name.table.0.clone(),
    }
}

/// The qualified name of a catalog table, for messages.
pub fn qualified(table: &Table) -> String {
    format!("{}.{}", table.database, table.name)
}

/// The column a statement names.
pub fn column<'t>(table: &'t Table, name: &Ident) -> Result<&'t Column, ResolveError> {
    table
        .columns
        .iter()
        .find(|column| same(&column.name, &name.0))
        .ok_or_else(|| ResolveError::UnknownColumn {
            name: name.0.clone(),
            table: qualified(table),
            suggestion: closest(
                &name.0,
                table.columns.iter().map(|column| column.name.as_str()),
            ),
        })
}

/// The candidate within a small edit distance of `name`, if any.
pub fn closest<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> Option<String> {
    let limit = (name.len() / 3).clamp(1, 3);
    candidates
        .map(|candidate| (edit_distance(name, candidate), candidate))
        .filter(|(distance, _)| *distance <= limit)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate.to_owned())
}

/// Levenshtein distance, case-insensitive.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.to_lowercase().chars().collect();
    let b: Vec<char> = b.to_lowercase().chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(ca != cb);
            current.push(substitution.min(previous[j + 1] + 1).min(current[j] + 1));
        }
        previous = current;
    }
    previous[b.len()]
}
