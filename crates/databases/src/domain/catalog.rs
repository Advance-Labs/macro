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
    Catalog, ColumnKind, ColumnSchema, DataType as StoredDataType, DatabaseSchema, EntityKind,
    OptionSchema, OptionValue, PropertyType as StoredType, Schema, TableSchema,
};
use models_permissions::share_permission::access_level::AccessLevel;
use models_properties::service::property_definition_with_options::PropertyDefinitionWithOptions;
use models_properties::service::property_option::PropertyOptionValue;
use models_properties::shared::DataType;
use uuid::Uuid;

use crate::domain::models::{
    Column, ColumnConfig, Database, DatabaseId, PropertyDefinitionId, Table, TableId,
};

#[cfg(test)]
mod test;

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

/// The entries as the schema the engine builds its catalog from, tables in
/// their order.
pub fn schema(entries: &[TableEntry]) -> Schema {
    let mut databases: Vec<DatabaseSchema> = Vec::new();
    for entry in entries {
        let table = TableSchema {
            id: entry.table.id,
            name: entry.table.name.clone(),
            columns: entry.columns.iter().map(column_schema).collect(),
        };
        match databases.last_mut() {
            Some(database) if database.id == entry.database.id => database.tables.push(table),
            _ => databases.push(DatabaseSchema {
                id: entry.database.id,
                name: entry.database.name.clone(),
                tables: vec![table],
            }),
        }
    }
    Schema {
        databases,
        platform: Vec::new(),
    }
}

/// The catalog a statement run from `scope` sees.
pub fn engine_catalog(entries: &[TableEntry], scope: Option<DatabaseId>) -> Catalog {
    database_sql::catalog::build(&schema(entries), scope)
}

/// One column as the schema describes it.
pub fn column_schema(column: &ColumnEntry) -> ColumnSchema {
    ColumnSchema {
        id: column.column.id,
        definition: column.definition.definition.id,
        name: column.name().to_owned(),
        property: PropertyType::of(&column.column, &column.definition).stored(),
        options: column
            .definition
            .property_options
            .iter()
            .map(|option| OptionSchema {
                id: option.id,
                value: option_value(&option.value),
                order: option.display_order,
            })
            .collect(),
    }
}

/// The engine's view of a column's type.
pub fn column_kind(column: &ColumnEntry) -> ColumnKind {
    column_schema(column).kind()
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
        self.stored().kind(Vec::new())
    }

    /// The type as the engine's schema spells it.
    pub fn stored(&self) -> StoredType {
        StoredType {
            data_type: match self.data_type {
                DataType::String => StoredDataType::String,
                DataType::Number => StoredDataType::Number,
                DataType::Boolean => StoredDataType::Boolean,
                DataType::Date => StoredDataType::Date,
                DataType::Link => StoredDataType::Link,
                DataType::SelectString => StoredDataType::SelectString,
                DataType::SelectNumber => StoredDataType::SelectNumber,
                DataType::Tag => StoredDataType::Tag,
                DataType::Entity => StoredDataType::Entity,
            },
            multi: self.is_multi_select,
            entity_type: self.specific_entity_type.map(entity_kind),
            relation: self.relation,
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

/// An option's value as the engine's schema holds it.
pub fn option_value(value: &PropertyOptionValue) -> OptionValue {
    match value {
        PropertyOptionValue::String(text) => OptionValue::String(text.clone()),
        PropertyOptionValue::Number(number) => OptionValue::Number(*number),
    }
}

/// An option's label as users write it.
pub fn option_display(value: &PropertyOptionValue) -> String {
    option_value(value).label()
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
