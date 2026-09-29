//! What the caller can see: tables, their columns, and the columns' types and
//! select options. Built once per request by the caller from the databases
//! the viewer has access to; a table that is not in the catalog does not exist
//! as far as a query is concerned. A driver builds it in JSON, in camel
//! case, and hands it across the wasm boundary.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Every table a statement may name.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    /// The visible tables.
    pub tables: Vec<Table>,
}

/// One table and its columns, in display order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    /// The table id.
    pub id: Uuid,
    /// The database the table belongs to, as users name it.
    pub database: String,
    /// The table's name, as users name it.
    pub name: String,
    /// The columns, in display order.
    pub columns: Vec<Column>,
    /// Where its rows come from.
    #[serde(default)]
    pub source: TableSource,
}

/// Where a table's rows come from. The engine only says which; the driver
/// serving its fetch requests decides how.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TableSource {
    /// A Macro database table, read through Soup.
    #[default]
    Database,
    /// The people the viewer can see: `id`, `name`, `email`.
    People,
}

/// The id of the `people` table. Platform tables have fixed ids, so a saved
/// query keeps meaning the same thing.
pub const PEOPLE_TABLE: Uuid = Uuid::from_u128(0x6d61_6372_6f00_0000_0000_0000_7065_6f70);
/// `people.id`: the user's entity id.
pub const PEOPLE_ID: Uuid = Uuid::from_u128(0x6d61_6372_6f00_0000_0000_0000_7065_6f71);
/// `people.name`.
pub const PEOPLE_NAME: Uuid = Uuid::from_u128(0x6d61_6372_6f00_0000_0000_0000_7065_6f72);
/// `people.email`.
pub const PEOPLE_EMAIL: Uuid = Uuid::from_u128(0x6d61_6372_6f00_0000_0000_0000_7065_6f73);

/// The `macro.people` table: every person the viewer can see, keyed by
/// entity id so entity columns join to it. Its `id` cells are
/// [`crate::fold::Cell::Entities`] with one id each.
pub fn people_table() -> Table {
    Table {
        id: PEOPLE_TABLE,
        database: "macro".into(),
        name: "people".into(),
        columns: vec![
            Column {
                id: PEOPLE_ID,
                name: "id".into(),
                kind: ColumnKind::Entity { multi: false },
            },
            Column {
                id: PEOPLE_NAME,
                name: "name".into(),
                kind: ColumnKind::Text,
            },
            Column {
                id: PEOPLE_EMAIL,
                name: "email".into(),
                kind: ColumnKind::Text,
            },
        ],
        source: TableSource::People,
    }
}

/// One column: a property definition bound to the table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    /// The property definition id.
    pub id: Uuid,
    /// The column's display name.
    pub name: String,
    /// What the column holds.
    pub kind: ColumnKind,
}

/// The value type of a column, mirroring the property data types a query can
/// compare against. The static string form is how the kind reads in an
/// error message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::IntoStaticStr)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[strum(serialize_all = "lowercase")]
pub enum ColumnKind {
    /// Free text.
    Text,
    /// A number.
    Number,
    /// A checkbox.
    #[strum(serialize = "checkbox")]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
        self.into()
    }
}
