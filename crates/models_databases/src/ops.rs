//! Ops: the whole data-write surface of a database, and what each one did.

#[cfg(test)]
mod test;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ids::{ColumnId, DatabaseId, RowId, TableId, TableVersion};

/// One write to a database's data. A request's ops apply together or not at
/// all, and every op names a table of the database the request is for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DatabaseOp {
    /// Append rows to a table, in order, each with the cells it starts with.
    #[serde(rename_all = "camelCase")]
    InsertRows {
        /// The table.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// One entry per new row: the cells it starts with. Columns left out
        /// start empty.
        rows: Vec<Vec<CellWrite>>,
        /// Create a select option for a label the column does not have yet,
        /// instead of refusing the op.
        #[serde(default)]
        create_missing_options: bool,
    },
    /// Write cells of existing rows. Last write wins: there is no version
    /// check.
    #[serde(rename_all = "camelCase")]
    UpdateRows {
        /// The table the rows belong to.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// Which rows get which cells.
        changes: RowChanges,
        /// Create a select option for a label the column does not have yet,
        /// instead of refusing the op.
        #[serde(default)]
        create_missing_options: bool,
    },
    /// Remove rows and their cells.
    DeleteRows {
        /// The table the rows belong to.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// The rows, each named once.
        #[schema(value_type = Vec<Uuid>)]
        rows: Vec<RowId>,
    },
    /// Convert a column to another type, converting its cells. A value that
    /// does not fit refuses the change unless `clearInvalid` empties it.
    #[serde(rename_all = "camelCase")]
    ChangeColumnType {
        /// The table.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// The column placement; its id survives the change.
        #[schema(value_type = Uuid)]
        column: ColumnId,
        /// The type it becomes.
        to: ColumnKind,
        /// Empty the cells whose value does not fit, instead of refusing; a
        /// cell with several values going to a single-valued type keeps its
        /// first.
        #[serde(default)]
        clear_invalid: bool,
    },
}

/// One cell of a row: which column, and its new value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema, specta::Type)]
pub struct CellWrite {
    /// The column placement.
    #[schema(value_type = Uuid)]
    pub column: ColumnId,
    /// The value, or [`CellValue::Clear`] to empty the cell.
    pub value: CellValue,
}

/// Which rows an update writes, and with what.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RowChanges {
    /// The same cells on every row.
    Uniform {
        /// The rows.
        #[schema(value_type = Vec<Uuid>)]
        rows: Vec<RowId>,
        /// The cells each of them gets.
        cells: Vec<CellWrite>,
    },
    /// Each row its own cells.
    PerRow {
        /// The rows and their cells, in order.
        rows: Vec<RowChange>,
    },
}

/// One row's cells in a [`RowChanges::PerRow`] update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema, specta::Type)]
pub struct RowChange {
    /// The row.
    #[schema(value_type = Uuid)]
    pub row: RowId,
    /// Its new cells.
    pub cells: Vec<CellWrite>,
}

/// A cell's value. It must fit the column's type: text for a text column,
/// options of the column for a select, and so on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema, specta::Type)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum CellValue {
    /// Free text.
    Text(String),
    /// A finite number.
    Number(f64),
    /// A checkbox.
    Boolean(bool),
    /// A date-time.
    Date(DateTime<Utc>),
    /// Complete http or https URLs; at most one for a single-valued column.
    Link(Vec<String>),
    /// Options of a select or tag column; at most one for a single-valued
    /// column.
    Options(Vec<OptionRef>),
    /// References to Macro entities of the kind the column points at; at
    /// most one for a single-valued column.
    Entities(Vec<EntityRef>),
    /// Rows of the table a relation column points at, by [`RowId`].
    Rows(Vec<Uuid>),
    /// No value: the cell is emptied.
    Clear,
}

/// A select option, by its id or by its label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum OptionRef {
    /// An option the column has.
    Id(Uuid),
    /// An option's label, matched without regard to case. An unknown label
    /// is refused unless the op creates missing options.
    Label(String),
}

/// A reference to one Macro entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EntityRef {
    /// What kind of entity it is; it must be the kind the column points at.
    pub entity_type: EntityKind,
    /// The entity's id.
    pub entity_id: String,
}

/// A kind of Macro entity a reference column can point at.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema, specta::Type,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EntityKind {
    /// People.
    User,
    /// Documents.
    Document,
    /// Tasks.
    Task,
    /// CRM companies.
    Company,
    /// Call recordings.
    CallRecord,
    /// Channels.
    Channel,
    /// AI chats.
    Chat,
    /// Projects.
    Project,
    /// Email threads.
    Thread,
    /// Calendar events.
    CalendarEvent,
    /// Initiatives.
    Initiative,
}

/// A type a column can have.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema, specta::Type,
)]
#[serde(tag = "type", rename_all = "snake_case")]
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
    /// Text options.
    Select {
        /// Whether a cell holds several options.
        multi: bool,
    },
    /// Numeric options.
    SelectNumber {
        /// Whether a cell holds several options.
        multi: bool,
    },
    /// Colored labels; always several per cell.
    Tag,
    /// References to Macro entities.
    Entity {
        /// What the references point at.
        target: EntityKind,
        /// Whether a cell holds several references.
        multi: bool,
    },
    /// Rows of another table.
    Relation {
        /// The database of the related table.
        #[schema(value_type = Uuid)]
        database: DatabaseId,
        /// The related table.
        #[schema(value_type = Uuid)]
        table: TableId,
    },
}

/// What one op did, in the order the ops were sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OpResult {
    /// What an insert, update or delete did.
    #[serde(rename_all = "camelCase")]
    RowsWritten {
        /// The table's version once the request committed.
        table_version: TableVersion,
        /// The rows an insert created, in the order they were sent; empty
        /// for an update or a delete.
        #[schema(value_type = Vec<Uuid>)]
        inserted: Vec<RowId>,
        /// How many rows the op inserted, updated or deleted.
        affected: u32,
    },
    /// What a column type change did.
    #[serde(rename_all = "camelCase")]
    ColumnTyped {
        /// The table's version after the change.
        table_version: TableVersion,
        /// Cells emptied because their value did not fit the new type.
        cleared_cells: u32,
        /// Cells that held several values and kept only their first.
        trimmed_cells: u32,
    },
}
