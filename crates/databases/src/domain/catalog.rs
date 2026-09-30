//! The viewer's catalog: every table they can see, with its columns bound to
//! their definitions, and the same catalog in the shape the SQL engine
//! reads.
//!
//! Names are the display names. A statement writes `crm.deals` and `"Due
//! Date"`; the engine matches them case-insensitively and suggests the
//! closest name on a miss, so there is no separate SQL-name layer any more.

use std::collections::HashMap;

use database_sql::catalog::{
    Catalog, Column as EngineColumn, ColumnKind, SelectOption, Table as EngineTable, TableSource,
};
use models_properties::service::property_definition_with_options::PropertyDefinitionWithOptions;
use models_properties::service::property_option::PropertyOptionValue;
use models_properties::shared::DataType;
use uuid::Uuid;

use crate::domain::models::{
    AccessGrant, Column, ColumnConfig, Database, DatabaseId, PropertyDefinitionId, Table, TableId,
};

/// One table the viewer can see, with what the service needs to run SQL
/// against it and to describe it.
#[derive(Debug, Clone)]
pub struct TableEntry {
    /// The database the table belongs to.
    pub database: Database,
    /// The table.
    pub table: Table,
    /// The viewer's grant on the database.
    pub grant: AccessGrant,
    /// The columns, in display order. Lookup columns are not here: they are
    /// derived and have no cells.
    pub columns: Vec<ColumnEntry>,
}

/// One column placement with the definition behind it.
#[derive(Debug, Clone)]
pub struct ColumnEntry {
    /// The placement.
    pub column: Column,
    /// The definition behind it.
    pub definition: PropertyDefinitionWithOptions,
    /// Whether SQL may write this column.
    pub writable: bool,
}

impl ColumnEntry {
    /// The name the column goes by: the placement's own, else the
    /// definition's.
    pub fn name(&self) -> &str {
        self.column
            .display_name
            .as_deref()
            .unwrap_or(&self.definition.definition.display_name)
    }

    /// Whether the column is a relation to rows of another table.
    pub fn is_relation(&self) -> bool {
        matches!(self.column.config, Some(ColumnConfig::Link { .. }))
    }

    /// Whether a cell holds several values.
    pub fn is_multi(&self) -> bool {
        self.definition.definition.is_multi_select || self.is_relation()
    }
}

impl TableEntry {
    /// The column bound to a definition.
    pub fn column_for(&self, definition: PropertyDefinitionId) -> Option<&ColumnEntry> {
        self.columns
            .iter()
            .find(|column| column.definition.definition.id == definition)
    }
}

/// Assemble the viewer's entries from what the repository returned.
pub fn build_entries(
    databases: &[Database],
    tables: &[Table],
    columns: &[Column],
    definitions: &HashMap<PropertyDefinitionId, PropertyDefinitionWithOptions>,
    grants: &HashMap<DatabaseId, AccessGrant>,
) -> Vec<TableEntry> {
    let databases_by_id: HashMap<DatabaseId, &Database> =
        databases.iter().map(|d| (d.id, d)).collect();
    let mut columns_by_table: HashMap<TableId, Vec<&Column>> = HashMap::new();
    for column in columns {
        columns_by_table
            .entry(column.table_id)
            .or_default()
            .push(column);
    }
    tables
        .iter()
        .filter_map(|table| {
            let grant = *grants.get(&table.database_id)?;
            let database = *databases_by_id.get(&table.database_id)?;
            let writable = grant.can_write();
            let columns = columns_by_table
                .get(&table.id)
                .into_iter()
                .flatten()
                .filter(|column| !matches!(column.config, Some(ColumnConfig::Lookup { .. })))
                .filter_map(|column| {
                    let definition = definitions.get(&column.property_definition_id)?.clone();
                    Some(ColumnEntry {
                        column: (*column).clone(),
                        definition,
                        writable,
                    })
                })
                .collect();
            Some(TableEntry {
                database: database.clone(),
                table: table.clone(),
                grant,
                columns,
            })
        })
        .collect()
}

/// Keep the statement addressable from `scope`: a table of another database
/// whose database and table names both match one of the scoped database's
/// (case-insensitively, as the engine matches) is dropped, so the scoped
/// table wins instead of the statement being ambiguous.
pub fn scope_entries(entries: &mut Vec<TableEntry>, scope: DatabaseId) {
    let scoped: Vec<(String, String)> = entries
        .iter()
        .filter(|entry| entry.database.id == scope)
        .map(|entry| {
            (
                entry.database.name.to_lowercase(),
                entry.table.name.to_lowercase(),
            )
        })
        .collect();
    entries.retain(|entry| {
        entry.database.id == scope
            || !scoped.contains(&(
                entry.database.name.to_lowercase(),
                entry.table.name.to_lowercase(),
            ))
    });
}

/// The entries as the engine's catalog.
pub fn engine_catalog(entries: &[TableEntry]) -> Catalog {
    Catalog {
        tables: entries
            .iter()
            .map(|entry| EngineTable {
                id: entry.table.id,
                database: entry.database.name.clone(),
                name: entry.table.name.clone(),
                source: TableSource::Database,
                columns: entry
                    .columns
                    .iter()
                    .map(|column| EngineColumn {
                        id: column.definition.definition.id,
                        name: column.name().to_owned(),
                        kind: column_kind(column),
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// The engine's view of a column's type.
pub fn column_kind(column: &ColumnEntry) -> ColumnKind {
    let definition = &column.definition.definition;
    if column.is_relation() {
        return ColumnKind::Entity { multi: true };
    }
    match definition.data_type {
        DataType::String => ColumnKind::Text,
        DataType::Number => ColumnKind::Number,
        DataType::Boolean => ColumnKind::Boolean,
        DataType::Date => ColumnKind::Date,
        DataType::Link => ColumnKind::Link,
        DataType::SelectString | DataType::SelectNumber | DataType::Tag => ColumnKind::Select {
            multi: definition.is_multi_select,
            options: option_labels(&column.definition)
                .into_iter()
                .map(|(id, label)| SelectOption { id, label })
                .collect(),
        },
        DataType::Entity => ColumnKind::Entity {
            multi: definition.is_multi_select,
        },
    }
}

/// A display name as SQL spells it: always double-quoted, an embedded quote
/// doubled, so a client never has to know which names need quoting.
pub fn sql_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// A table as SQL names it: qualified by its database, so every new
/// database's `Table 1` is its own table.
pub fn sql_table_name(database: &str, table: &str) -> String {
    format!("{}.{}", sql_identifier(database), sql_identifier(table))
}

/// An option's label as users write it.
pub fn option_display(value: &PropertyOptionValue) -> String {
    match value {
        PropertyOptionValue::String(text) => text.clone(),
        PropertyOptionValue::Number(number) => format_number(*number),
    }
}

/// Every option of a definition with its label, in display order.
pub fn option_labels(definition: &PropertyDefinitionWithOptions) -> Vec<(Uuid, String)> {
    let mut options: Vec<_> = definition.property_options.iter().collect();
    options.sort_by_key(|option| option.display_order);
    options
        .into_iter()
        .map(|option| (option.id, option_display(&option.value)))
        .collect()
}

/// A number the way a label shows it: no trailing `.0` on whole numbers.
pub fn format_number(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{}", number as i64)
    } else {
        number.to_string()
    }
}
