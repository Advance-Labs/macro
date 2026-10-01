//! The viewer's catalog: every table they can see, with its columns bound to
//! their definitions.

use std::collections::HashMap;

use models_databases::cast::{Cast, CastKind, Contents, TARGETS, cast, number_label};
use models_databases::{ColumnKind, EntityKind};
use models_permissions::share_permission::access_level::AccessLevel;
use models_properties::service::property_definition_with_options::PropertyDefinitionWithOptions;
use models_properties::service::property_option::PropertyOptionValue;
use models_properties::shared::DataType;
use uuid::Uuid;

use crate::domain::models::{
    Column, ColumnConfig, Database, DatabaseId, PropertyDefinitionId, Table, TableId,
};

/// One table the viewer can see, with what the service needs to write and
/// describe it.
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
    /// Whether the viewer may write this column.
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

    /// The property type a [`ColumnKind`] names.
    pub fn from_column_kind(kind: ColumnKind) -> Self {
        let plain = |data_type, is_multi_select| PropertyType {
            data_type,
            is_multi_select,
            specific_entity_type: None,
            relation: false,
        };
        match kind {
            ColumnKind::Text => plain(DataType::String, false),
            ColumnKind::Number => plain(DataType::Number, false),
            ColumnKind::Boolean => plain(DataType::Boolean, false),
            ColumnKind::Date => plain(DataType::Date, false),
            ColumnKind::Link => plain(DataType::Link, false),
            ColumnKind::Select { multi } => plain(DataType::SelectString, multi),
            ColumnKind::SelectNumber { multi } => plain(DataType::SelectNumber, multi),
            ColumnKind::Tag => plain(DataType::Tag, true),
            ColumnKind::Entity { target, multi } => PropertyType {
                specific_entity_type: Some(entity_type(target)),
                ..plain(DataType::Entity, multi)
            },
            ColumnKind::Relation { .. } => PropertyType::RELATION,
        }
    }

    /// A column of this type's values, as the cast rule reads them.
    pub fn cast_kind(&self) -> CastKind {
        if self.relation {
            return CastKind::Relation;
        }
        match self.data_type {
            DataType::String => CastKind::Text,
            DataType::Number => CastKind::Number,
            DataType::Boolean => CastKind::Boolean,
            DataType::Date => CastKind::Date,
            DataType::Link => CastKind::Link,
            DataType::SelectString | DataType::SelectNumber | DataType::Tag => CastKind::Select {
                multi: self.is_multi_select,
            },
            DataType::Entity => match self.specific_entity_type.map(entity_kind) {
                Some(None) => CastKind::Relation,
                target => CastKind::Entity {
                    target: target.flatten().unwrap_or(EntityKind::User),
                    multi: self.is_multi_select,
                },
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
) -> (Vec<ColumnKind>, Vec<ColumnKind>) {
    let current = PropertyType::of(column, definition);
    let from = current.cast_kind();
    let mut safe = Vec::new();
    let mut checked = Vec::new();
    for target in TARGETS {
        if PropertyType::from_column_kind(target) == current {
            continue;
        }
        match cast(from, CastKind::from(target), Contents::Filled) {
            Cast::Safe => safe.push(target),
            Cast::Checked => checked.push(target),
            Cast::Never(_) => {}
        }
    }
    (safe, checked)
}

/// What a reference column points at, as ops name it; `None` for rows of
/// another table, which a relation holds.
pub fn entity_kind(entity_type: models_properties::EntityType) -> Option<EntityKind> {
    use models_properties::EntityType as Stored;
    Some(match entity_type {
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
        Stored::DatabaseRow => return None,
    })
}

/// The properties system's name for what a reference points at.
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
        PropertyOptionValue::Number(number) => number_label(*number),
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
