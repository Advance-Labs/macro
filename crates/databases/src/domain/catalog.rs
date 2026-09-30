//! The viewer's catalog: every table they can see, with its columns bound to
//! their definitions, and the same catalog in the shape the SQL engine
//! reads.
//!
//! Names are the display names. A statement writes `crm.deals` and `"Due
//! Date"`; the engine matches them case-insensitively and suggests the
//! closest name on a miss, so there is no separate SQL-name layer any more.

use std::collections::HashMap;

use database_sql::cast::{Cast, ColumnType, Contents, TARGETS, cast};
use database_sql::catalog::{
    Catalog, Column as EngineColumn, ColumnKind, EntityKind, SelectOption, Table as EngineTable,
    TableSource,
};
use models_permissions::share_permission::access_level::AccessLevel;
use models_properties::service::property_definition_with_options::PropertyDefinitionWithOptions;
use models_properties::service::property_option::PropertyOptionValue;
use models_properties::shared::DataType;
use uuid::Uuid;

use crate::domain::models::{
    Column, ColumnConfig, Database, DatabaseId, PropertyDefinitionId, Table, TableId,
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
    pub grant: AccessLevel,
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
    grants: &HashMap<DatabaseId, AccessLevel>,
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
            let writable = grant >= AccessLevel::Edit;
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
    match PropertyType::of(&column.column, &column.definition).kind() {
        ColumnKind::Select { multi, .. } => ColumnKind::Select {
            multi,
            options: option_labels(&column.definition)
                .into_iter()
                .map(|(id, label)| SelectOption { id, label })
                .collect(),
        },
        kind => kind,
    }
}

/// A column's type as the properties system stores it: what a column is,
/// or what a type change asks it to become.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropertyType {
    /// The property type.
    pub data_type: DataType,
    /// Whether a cell holds several values.
    pub is_multi_select: bool,
    /// What a reference column points at; `None` for a relation.
    pub specific_entity_type: Option<models_properties::EntityType>,
    /// Whether the column relates rows of another table.
    pub relation: bool,
}

impl PropertyType {
    /// A relation to some table's rows.
    pub const RELATION: PropertyType = PropertyType {
        data_type: DataType::Entity,
        is_multi_select: true,
        specific_entity_type: None,
        relation: true,
    };

    /// The type of a column placement bound to `definition`.
    pub fn of(column: &Column, definition: &PropertyDefinitionWithOptions) -> Self {
        let relation = matches!(column.config, Some(ColumnConfig::Link { .. }));
        let definition = &definition.definition;
        PropertyType {
            data_type: definition.data_type,
            is_multi_select: definition.is_multi_select || relation,
            specific_entity_type: definition.specific_entity_type.filter(|_| !relation),
            relation,
        }
    }

    /// The property type SQL's `ColumnType` names.
    pub fn from_column_type(column_type: ColumnType) -> Self {
        let plain = |data_type, is_multi_select| PropertyType {
            data_type,
            is_multi_select,
            specific_entity_type: None,
            relation: false,
        };
        match column_type {
            ColumnType::Text => plain(DataType::String, false),
            ColumnType::Number => plain(DataType::Number, false),
            ColumnType::Boolean => plain(DataType::Boolean, false),
            ColumnType::Date => plain(DataType::Date, false),
            ColumnType::Link => plain(DataType::Link, false),
            ColumnType::Select { multi } => plain(DataType::SelectString, multi),
            ColumnType::SelectNumber { multi } => plain(DataType::SelectNumber, multi),
            ColumnType::Tag => plain(DataType::Tag, true),
            ColumnType::Entity { target, multi } => PropertyType {
                specific_entity_type: Some(entity_type(target)),
                ..plain(DataType::Entity, multi)
            },
        }
    }

    /// The engine's kind for a column of this type, without options.
    pub fn kind(&self) -> ColumnKind {
        if self.relation {
            return ColumnKind::Entity {
                multi: true,
                target: EntityKind::Row,
            };
        }
        match self.data_type {
            DataType::String => ColumnKind::Text,
            DataType::Number => ColumnKind::Number,
            DataType::Boolean => ColumnKind::Boolean,
            DataType::Date => ColumnKind::Date,
            DataType::Link => ColumnKind::Link,
            DataType::SelectString | DataType::SelectNumber | DataType::Tag => ColumnKind::Select {
                multi: self.is_multi_select,
                options: Vec::new(),
            },
            DataType::Entity => ColumnKind::Entity {
                multi: self.is_multi_select,
                target: entity_kind(
                    self.specific_entity_type
                        .unwrap_or(models_properties::EntityType::User),
                ),
            },
        }
    }
}

/// The menu's types a column holding values can change to: those every
/// value survives, and those whose values are checked first. Its own type
/// is in neither.
pub fn cast_targets(
    column: &Column,
    definition: &PropertyDefinitionWithOptions,
) -> (Vec<ColumnType>, Vec<ColumnType>) {
    let current = PropertyType::of(column, definition);
    let from = current.kind();
    let mut safe = Vec::new();
    let mut checked = Vec::new();
    for target in TARGETS {
        if PropertyType::from_column_type(target) == current {
            continue;
        }
        match cast(&from, &target.kind(), Contents::Filled) {
            Cast::Safe => safe.push(target),
            Cast::Checked => checked.push(target),
            Cast::Never(_) => {}
        }
    }
    (safe, checked)
}

/// The engine's name for what an entity column references.
pub fn entity_kind(entity_type: models_properties::EntityType) -> EntityKind {
    use models_properties::EntityType as Stored;
    match entity_type {
        Stored::User => EntityKind::User,
        Stored::Document => EntityKind::Document,
        Stored::Task => EntityKind::Task,
        Stored::Company => EntityKind::Company,
        Stored::CallRecord => EntityKind::CallRecord,
        Stored::Channel => EntityKind::Channel,
        Stored::Chat => EntityKind::Chat,
        Stored::Project => EntityKind::Project,
        Stored::Thread => EntityKind::Thread,
        Stored::CalendarEvent => EntityKind::CalendarEvent,
        Stored::Initiative => EntityKind::Initiative,
        Stored::DatabaseRow => EntityKind::Row,
    }
}

/// The properties system's name for what an entity column references.
pub fn entity_type(kind: EntityKind) -> models_properties::EntityType {
    use models_properties::EntityType as Stored;
    match kind {
        EntityKind::User => Stored::User,
        EntityKind::Document => Stored::Document,
        EntityKind::Task => Stored::Task,
        EntityKind::Company => Stored::Company,
        EntityKind::CallRecord => Stored::CallRecord,
        EntityKind::Channel => Stored::Channel,
        EntityKind::Chat => Stored::Chat,
        EntityKind::Project => Stored::Project,
        EntityKind::Thread => Stored::Thread,
        EntityKind::CalendarEvent => Stored::CalendarEvent,
        EntityKind::Initiative => Stored::Initiative,
        EntityKind::Row => Stored::DatabaseRow,
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
