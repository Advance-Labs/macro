//! The catalog a statement runs against: every database the viewer can
//! reach, as the databases service details them, mapped onto the schema
//! `database_sql::catalog::build` takes. The browser maps the same details
//! onto the same schema, so a statement names the same tables there.

#[cfg(test)]
mod test;

use database_sql::catalog::{
    Catalog, Column, ColumnSchema, DataType as SchemaDataType, DatabaseSchema, EntityKind,
    OptionSchema, OptionValue, PlatformTable, PropertyType, Schema, TableSchema,
};
use databases::domain::models::{
    ColumnConfig, ColumnDetail, DatabaseDetail, DatabaseId, TableDetail, TableId,
};
use models_properties::service::property_option::PropertyOptionValue;
use models_properties::shared::DataType;
use uuid::Uuid;

/// What one statement can see: the viewer's databases in detail, and the
/// catalog built from them for the statement's scope.
pub(crate) struct ViewerCatalog {
    databases: Vec<DatabaseDetail>,
    catalog: Catalog,
}

impl ViewerCatalog {
    /// The catalog a statement written from `scope` sees.
    pub(crate) fn new(databases: Vec<DatabaseDetail>, scope: Option<DatabaseId>) -> Self {
        let catalog = database_sql::catalog::build(&schema(&databases), scope);
        Self { databases, catalog }
    }

    /// The engine's catalog.
    pub(crate) fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// Whether the viewer can reach this database.
    pub(crate) fn has_database(&self, database_id: DatabaseId) -> bool {
        self.databases
            .iter()
            .any(|detail| detail.database.id == database_id)
    }

    /// A table with the database it belongs to.
    pub(crate) fn table(&self, table_id: TableId) -> Option<(&DatabaseDetail, &TableDetail)> {
        self.databases.iter().find_map(|database| {
            database
                .tables
                .iter()
                .find(|table| table.table.id == table_id)
                .map(|table| (database, table))
        })
    }

    /// The table a relation column's rows belong to, found by the column's
    /// definition.
    pub(crate) fn related_table(&self, definition: Uuid) -> Option<TableId> {
        self.databases
            .iter()
            .flat_map(|database| &database.tables)
            .flat_map(|table| &table.columns)
            .find(|column| column.definition.definition.id == definition)
            .and_then(|column| match column.column.config {
                Some(ColumnConfig::Link { table_id, .. }) => Some(table_id),
                _ => None,
            })
    }

    /// The engine's view of a column, found by the definition reads key it
    /// by. A definition shared by several tables has one kind everywhere.
    pub(crate) fn column(&self, definition: Uuid) -> Option<&Column> {
        self.catalog
            .tables
            .iter()
            .flat_map(|table| &table.columns)
            .find(|column| column.id == definition)
    }
}

/// The databases as the schema the engine builds its catalog from, with the
/// people the viewer can see.
pub(crate) fn schema(databases: &[DatabaseDetail]) -> Schema {
    Schema {
        databases: databases
            .iter()
            .map(|detail| DatabaseSchema {
                id: detail.database.id,
                name: detail.database.name.clone(),
                tables: detail
                    .tables
                    .iter()
                    .map(|table| TableSchema {
                        id: table.table.id,
                        name: table.table.name.clone(),
                        columns: table
                            .columns
                            .iter()
                            // Lookups are derived and have no cells.
                            .filter(|column| {
                                !matches!(column.column.config, Some(ColumnConfig::Lookup { .. }))
                            })
                            .map(column_schema)
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
        platform: vec![PlatformTable::People],
    }
}

fn column_schema(column: &ColumnDetail) -> ColumnSchema {
    let definition = &column.definition.definition;
    ColumnSchema {
        id: column.column.id,
        definition: definition.id,
        name: column
            .column
            .display_name
            .clone()
            .unwrap_or_else(|| definition.display_name.clone()),
        property: PropertyType {
            data_type: data_type(definition.data_type),
            multi: definition.is_multi_select,
            entity_type: definition.specific_entity_type.map(entity_kind),
            relation: matches!(column.column.config, Some(ColumnConfig::Link { .. })),
        },
        options: column
            .definition
            .property_options
            .iter()
            .map(|option| OptionSchema {
                id: option.id,
                value: match &option.value {
                    PropertyOptionValue::String(text) => OptionValue::String(text.clone()),
                    PropertyOptionValue::Number(number) => OptionValue::Number(*number),
                },
                order: option.display_order,
            })
            .collect(),
    }
}

fn data_type(data_type: DataType) -> SchemaDataType {
    match data_type {
        DataType::String => SchemaDataType::String,
        DataType::Number => SchemaDataType::Number,
        DataType::Boolean => SchemaDataType::Boolean,
        DataType::Date => SchemaDataType::Date,
        DataType::Link => SchemaDataType::Link,
        DataType::SelectString => SchemaDataType::SelectString,
        DataType::SelectNumber => SchemaDataType::SelectNumber,
        DataType::Tag => SchemaDataType::Tag,
        DataType::Entity => SchemaDataType::Entity,
    }
}

fn entity_kind(entity_type: models_properties::EntityType) -> EntityKind {
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
