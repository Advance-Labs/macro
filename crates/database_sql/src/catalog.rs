//! What the caller can see: tables, their columns, and the columns' types and
//! select options. Built once per request by the caller from the databases
//! the viewer has access to; a table that is not in the catalog does not exist
//! as far as a query is concerned.

use uuid::Uuid;

/// Every table a statement may name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Catalog {
    /// The visible tables.
    pub tables: Vec<Table>,
}

/// One table and its columns, in display order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// The table id.
    pub id: Uuid,
    /// The database the table belongs to, as users name it.
    pub database: String,
    /// The table's name, as users name it.
    pub name: String,
    /// The columns, in display order.
    pub columns: Vec<Column>,
}

/// One column: a property definition bound to the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// The property definition id.
    pub id: Uuid,
    /// The column's display name.
    pub name: String,
    /// What the column holds.
    pub kind: ColumnKind,
}

/// The value type of a column, mirroring the property data types a query can
/// compare against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnKind {
    /// Free text.
    Text,
    /// A number.
    Number,
    /// A checkbox.
    Boolean,
    /// A date-time.
    Date,
    /// A URL.
    Link,
    /// One or more of a fixed set of options.
    Select {
        /// Whether a cell holds several options.
        multi: bool,
        /// The options, in display order.
        options: Vec<SelectOption>,
    },
    /// One or more references to Macro entities.
    Entity {
        /// Whether a cell holds several references.
        multi: bool,
    },
}

/// One option of a select column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectOption {
    /// The option id.
    pub id: Uuid,
    /// The label users type in SQL.
    pub label: String,
}

impl ColumnKind {
    /// Whether a cell can hold several values.
    pub fn is_multi(&self) -> bool {
        matches!(
            self,
            ColumnKind::Select { multi: true, .. } | ColumnKind::Entity { multi: true }
        )
    }

    /// How the kind reads in an error message.
    pub fn describe(&self) -> &'static str {
        match self {
            ColumnKind::Text => "text",
            ColumnKind::Number => "number",
            ColumnKind::Boolean => "checkbox",
            ColumnKind::Date => "date",
            ColumnKind::Link => "link",
            ColumnKind::Select { .. } => "select",
            ColumnKind::Entity { .. } => "entity",
        }
    }
}
