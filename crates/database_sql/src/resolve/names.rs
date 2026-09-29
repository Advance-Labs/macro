//! Looking names up in the catalog, case-insensitively, with a suggestion
//! when nothing matches. A [`Scope`] is the relations a `SELECT` reads and
//! the keys it hands out for their columns.

use uuid::Uuid;

use crate::catalog::{Catalog, Column, ColumnKind, Table};
use crate::parse::{ColumnRef, FromItem, Ident, TableName};

use super::{Binding, ResolveError, column_key, row_id_key};

/// The name of a table's row id column.
pub const ROW_ID: &str = "row_id";

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

/// The column a statement names in one table.
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

/// One relation of a `SELECT`: a table and the alias qualifying its columns.
pub struct ScopeRelation<'c> {
    /// The alias: the one written, else the table name.
    pub alias: String,
    /// The table.
    pub table: &'c Table,
}

/// The relations a `SELECT` reads, and every key handed out for their
/// columns.
pub struct Scope<'c> {
    /// The relations, `FROM` first.
    pub relations: Vec<ScopeRelation<'c>>,
    /// Every column bound so far, first use first.
    pub bindings: Vec<Binding>,
}

/// A column reference bound to a relation.
#[derive(Debug, Clone)]
pub struct Bound {
    /// The key later stages use.
    pub key: Uuid,
    /// The relation the column belongs to.
    pub relation: usize,
    /// The column; for `row_id`, a stand-in entity column named `row_id`.
    pub column: Column,
    /// The property definition; `None` for `row_id`.
    pub definition: Option<Uuid>,
}

impl<'c> Scope<'c> {
    /// A scope with only the `FROM` table.
    pub fn new(catalog: &'c Catalog, from: &FromItem) -> Result<Self, ResolveError> {
        let mut scope = Scope {
            relations: Vec::new(),
            bindings: Vec::new(),
        };
        scope.add(catalog, from)?;
        Ok(scope)
    }

    /// Bring one more table into scope; answers its relation index.
    pub fn add(&mut self, catalog: &'c Catalog, item: &FromItem) -> Result<usize, ResolveError> {
        let table = table(catalog, &item.table)?;
        let alias = item
            .alias
            .as_ref()
            .map_or_else(|| table.name.clone(), |alias| alias.0.clone());
        if let Some(taken) = self
            .relations
            .iter()
            .find(|relation| same(&relation.alias, &alias))
        {
            return Err(ResolveError::DuplicateAlias {
                alias,
                table: qualified(taken.table),
            });
        }
        self.relations.push(ScopeRelation { alias, table });
        Ok(self.relations.len() - 1)
    }

    /// Whether the scope has a single relation.
    pub fn is_single(&self) -> bool {
        self.relations.len() == 1
    }

    /// The column a reference names, recorded in the bindings.
    pub fn column(&mut self, reference: &ColumnRef) -> Result<Bound, ResolveError> {
        let bound = self.lookup(reference)?;
        if !self.bindings.iter().any(|binding| binding.key == bound.key) {
            self.bindings.push(Binding {
                key: bound.key,
                relation: bound.relation,
                column: bound.definition,
            });
        }
        Ok(bound)
    }

    fn lookup(&self, reference: &ColumnRef) -> Result<Bound, ResolveError> {
        let candidates: Vec<usize> = match &reference.table {
            Some(alias) => {
                let index = self
                    .relations
                    .iter()
                    .position(|relation| same(&relation.alias, &alias.0))
                    .ok_or_else(|| ResolveError::UnknownAlias {
                        alias: alias.0.clone(),
                        column: reference.column.0.clone(),
                        relations: self.describe_relations(),
                    })?;
                vec![index]
            }
            None => (0..self.relations.len()).collect(),
        };

        let name = &reference.column.0;
        let found: Vec<Bound> = candidates
            .iter()
            .filter_map(|&index| self.bind(index, name))
            .collect();
        match found.len() {
            1 => Ok(found.into_iter().next().expect("one match")),
            0 => Err(self.unknown_column(&candidates, name)),
            _ => Err(ResolveError::AmbiguousColumn {
                name: name.clone(),
                qualified: found
                    .iter()
                    .map(|bound| format!("{}.{}", self.relations[bound.relation].alias, name))
                    .collect(),
            }),
        }
    }

    /// The column of that name in one relation, if it has one.
    fn bind(&self, index: usize, name: &str) -> Option<Bound> {
        let table = self.relations[index].table;
        if same(name, ROW_ID) {
            return Some(Bound {
                key: row_id_key(table.id),
                relation: index,
                column: Column {
                    id: row_id_key(table.id),
                    name: ROW_ID.into(),
                    kind: ColumnKind::Entity { multi: false },
                },
                definition: None,
            });
        }
        table
            .columns
            .iter()
            .find(|column| same(&column.name, name))
            .map(|column| Bound {
                key: column_key(index, column.id),
                relation: index,
                column: column.clone(),
                definition: Some(column.id),
            })
    }

    fn unknown_column(&self, candidates: &[usize], name: &str) -> ResolveError {
        let tables: Vec<&Table> = candidates
            .iter()
            .map(|&index| self.relations[index].table)
            .collect();
        ResolveError::UnknownColumn {
            name: name.to_owned(),
            table: tables
                .iter()
                .map(|table| qualified(table))
                .collect::<Vec<_>>()
                .join(" or "),
            suggestion: closest(
                name,
                tables
                    .iter()
                    .flat_map(|table| table.columns.iter().map(|column| column.name.as_str())),
            ),
        }
    }

    /// `crm.deals as d` for every relation, for messages.
    pub fn describe_relations(&self) -> Vec<String> {
        self.relations
            .iter()
            .map(|relation| format!("{} as {}", qualified(relation.table), relation.alias))
            .collect()
    }

    /// How a bound column reads in a message: `alias.column` when the scope
    /// has several relations, the bare name otherwise.
    pub fn describe(&self, bound: &Bound) -> String {
        if self.is_single() {
            bound.column.name.clone()
        } else {
            format!(
                "{}.{}",
                self.relations[bound.relation].alias, bound.column.name
            )
        }
    }
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
