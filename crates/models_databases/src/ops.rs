//! Ops: the whole write surface of a database, its schema and its data, and
//! what each one did.
//!
//! A request's ops apply in order, in one transaction, so a later op may name
//! what an earlier one created: tables, columns and options carry ids the
//! client mints (UUIDv7, `TableId::new()` and the like). An id that already
//! names something refuses the request. Rows keep server-minted ids, which
//! an insert's result answers.

#[cfg(test)]
mod test;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

use crate::ids::{ColumnId, DatabaseId, OptionId, PropertyId, RowId, TableId, TableVersion};
use crate::views::{
    CardPosition, DatabaseView, NewView, ViewId, ViewLayout, ViewPosition, ViewQuery,
};

/// One write to a database: its tables, columns, options, rows or views. A
/// request's ops apply in order and together, or not at all, and every op
/// names a table of the database the request is for (or, creating one, adds
/// it there).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DatabaseOp {
    /// Add a table, after the database's other tables. It starts with no
    /// columns and no rows.
    CreateTable {
        /// The new table's id, minted by the client; later ops of the
        /// request may name it.
        #[schema(value_type = Uuid)]
        id: TableId,
        /// Its name, unique within the database ignoring case.
        name: String,
    },
    /// Rename a table. Its id, columns and rows stay.
    #[serde(rename_all = "camelCase")]
    RenameTable {
        /// The table.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// Its new name, unique within the database ignoring case.
        name: String,
        /// The name the caller saw. Given, the rename is refused if the
        /// table goes by another one now, so a concurrent rename is not
        /// overwritten.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false)]
        #[specta(optional)]
        previous_name: Option<String>,
    },
    /// Remove a table with its columns, rows and views. A database keeps at
    /// least one table, and a table another table's relation points at
    /// stays until that relation goes.
    DeleteTable {
        /// The table.
        #[schema(value_type = Uuid)]
        table: TableId,
    },
    /// Set the order of the database's tables: `order` names every one of
    /// them once.
    ReorderTables {
        /// Every table, in its new order.
        #[schema(value_type = Vec<Uuid>)]
        order: Vec<TableId>,
    },
    /// Add a column to a table: a new property the database owns, or an
    /// existing one bound into the table.
    CreateColumn {
        /// The table.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// The new column's id, minted by the client; later ops of the
        /// request may name it.
        #[schema(value_type = Uuid)]
        id: ColumnId,
        /// What the column holds.
        definition: NewColumn,
        /// The column it goes right after; left out, it goes after the
        /// table's last column.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false, value_type = Option<Uuid>)]
        #[specta(optional)]
        after: Option<ColumnId>,
    },
    /// Rename a column. Its id, type and cells stay; SQL names it by its new
    /// name.
    #[serde(rename_all = "camelCase")]
    RenameColumn {
        /// The table.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// The column.
        #[schema(value_type = Uuid)]
        column: ColumnId,
        /// Its new name, unique within the table ignoring case.
        name: String,
        /// The name the caller saw. Given, the rename is refused if the
        /// column goes by another one now.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false)]
        #[specta(optional)]
        previous_name: Option<String>,
    },
    /// Remove a column and its cells. The views naming it forget it; a
    /// board grouped by it must go or regroup first. A property shared
    /// beyond the database stays, unbound here.
    DeleteColumn {
        /// The table.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// The column.
        #[schema(value_type = Uuid)]
        column: ColumnId,
    },
    /// Set the order of a table's columns: `order` names every one of them
    /// once.
    ReorderColumns {
        /// The table.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// Its columns, in their new order.
        #[schema(value_type = Vec<Uuid>)]
        order: Vec<ColumnId>,
    },
    /// Add options to a select or tag column, after its others. An option
    /// whose label the column already has, ignoring case, is left out, so
    /// re-sending a list adds only what is new. Like
    /// [`DatabaseOp::UpdateOption`], an option of a property shared beyond
    /// the database goes everywhere it is used.
    AddOptions {
        /// The table.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// The select or tag column.
        #[schema(value_type = Uuid)]
        column: ColumnId,
        /// The options, each under an id the client mints.
        options: Vec<NewOption>,
    },
    /// Append rows to a table, in order, each with the cells it starts with.
    #[serde(rename_all = "camelCase")]
    InsertRows {
        /// The table.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// One entry per new row: the cells it starts with. Columns left out
        /// start empty.
        rows: Vec<Vec<CellWrite>>,
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
    /// does not fit refuses the change, counting and quoting the misfits: a
    /// type change never empties a cell. To keep the original, create a
    /// column of the new type and write it the values that convert.
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
    },
    /// Relabel or recolour one option of a select or tag column. Every cell
    /// holding it keeps it. A column bound to a property shared outside the
    /// database changes wherever that property is used, so it takes the
    /// right to edit that property.
    UpdateOption {
        /// The table.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// The select or tag column.
        #[schema(value_type = Uuid)]
        column: ColumnId,
        /// The option.
        #[schema(value_type = Uuid)]
        option: OptionId,
        /// Its new label; left out, it keeps its own. Labels are unique
        /// within a column, ignoring case.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false)]
        #[specta(optional)]
        label: Option<String>,
        /// Its new colour, a hex string like `#RRGGBB`, or `null` to clear
        /// it; left out, it keeps its own. A tag option always has one.
        #[serde(
            default,
            deserialize_with = "present",
            skip_serializing_if = "Option::is_none"
        )]
        #[schema(value_type = Option<String>)]
        #[specta(type = Option<String>, optional)]
        color: Option<Option<String>>,
    },
    /// Remove one option of a select or tag column, and take it out of every
    /// cell holding it: a single-valued cell is emptied, a multi-valued one
    /// keeps its other options. Like [`DatabaseOp::UpdateOption`], an option
    /// of a shared property goes everywhere it is used.
    DeleteOption {
        /// The table.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// The select or tag column.
        #[schema(value_type = Uuid)]
        column: ColumnId,
        /// The option.
        #[schema(value_type = Uuid)]
        option: OptionId,
    },
    /// Add a view of the table, after its other views.
    CreateView {
        /// The table.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// What it shows and how.
        view: NewView,
    },
    /// Change a view's name, query or layout; what is left out stays.
    UpdateView {
        /// The view's table.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// The view.
        #[schema(value_type = Uuid)]
        view: ViewId,
        /// Its new name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false)]
        #[specta(optional)]
        name: Option<String>,
        /// Its new query.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false)]
        #[specta(optional)]
        query: Option<ViewQuery>,
        /// Its new layout. A board grouped by another column forgets where
        /// its cards were.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schema(nullable = false)]
        #[specta(optional)]
        layout: Option<ViewLayout>,
    },
    /// Remove a view, with where its cards were.
    DeleteView {
        /// The view's table.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// The view.
        #[schema(value_type = Uuid)]
        view: ViewId,
    },
    /// Set the order of a table's views: `order` names every one of them
    /// once.
    ReorderViews {
        /// The table.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// Its views, in their new order.
        #[schema(value_type = Vec<Uuid>)]
        order: Vec<ViewId>,
    },
    /// Move a board's card: into a lane, which sets the row's grouping cell
    /// to the lane's option (or empties it for the lane without one), and to
    /// a place there, between two of its cards. Only an unsorted board's
    /// cards move by hand.
    MoveCard {
        /// The view's table.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// The board.
        #[schema(value_type = Uuid)]
        view: ViewId,
        /// The card's row.
        #[schema(value_type = Uuid)]
        row: RowId,
        /// The lane it goes to: an option of the board's column, or `null`
        /// for the lane of cards without one.
        #[schema(required = true, value_type = Option<Uuid>)]
        lane: Option<OptionId>,
        /// The card that ends up just before it (it lands right after this
        /// one), if any.
        #[serde(default)]
        #[schema(value_type = Option<Uuid>)]
        before: Option<RowId>,
        /// The card that ends up just after it, if any. Given with `before`,
        /// it must be the card right after `before`; with neither, the card
        /// goes to the end of the lane.
        #[serde(default)]
        #[schema(value_type = Option<Uuid>)]
        after: Option<RowId>,
    },
}

impl DatabaseOp {
    /// The table the op names: the one it creates, for a creation; `None`
    /// for a reorder of the database's tables, which names them all.
    pub fn table(&self) -> Option<TableId> {
        match self {
            DatabaseOp::CreateTable { id, .. } => Some(*id),
            DatabaseOp::ReorderTables { .. } => None,
            DatabaseOp::RenameTable { table, .. }
            | DatabaseOp::DeleteTable { table }
            | DatabaseOp::CreateColumn { table, .. }
            | DatabaseOp::RenameColumn { table, .. }
            | DatabaseOp::DeleteColumn { table, .. }
            | DatabaseOp::ReorderColumns { table, .. }
            | DatabaseOp::AddOptions { table, .. }
            | DatabaseOp::InsertRows { table, .. }
            | DatabaseOp::UpdateRows { table, .. }
            | DatabaseOp::DeleteRows { table, .. }
            | DatabaseOp::ChangeColumnType { table, .. }
            | DatabaseOp::UpdateOption { table, .. }
            | DatabaseOp::DeleteOption { table, .. }
            | DatabaseOp::CreateView { table, .. }
            | DatabaseOp::UpdateView { table, .. }
            | DatabaseOp::DeleteView { table, .. }
            | DatabaseOp::ReorderViews { table, .. }
            | DatabaseOp::MoveCard { table, .. } => Some(*table),
        }
    }
}

/// What a new column holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema, specta::Type)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum NewColumn {
    /// A new property the database owns.
    #[serde(rename_all = "camelCase")]
    New {
        /// The column's name, unique within the table ignoring case.
        name: String,
        /// Its type. A relation names the table whose rows it holds, one the
        /// caller can see.
        #[serde(rename = "type")]
        kind: ColumnKind,
        /// For a select or tag column, the options it starts with, in
        /// order, each under an id the client mints. A select column with
        /// none accepts nothing until options are added.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[schema(nullable = false)]
        #[specta(optional)]
        options: Vec<NewOption>,
        /// Let the column's first value settle its type: only for a plain
        /// text column.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        #[specta(optional)]
        infer_type: bool,
    },
    /// An existing property, a person's, a team's or a system one, bound
    /// into the table under its own name.
    Existing {
        /// The property's definition.
        #[schema(value_type = Uuid)]
        property: PropertyId,
    },
}

/// A select or tag option to create.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema, specta::Type)]
pub struct NewOption {
    /// Its id, minted by the client; later ops of the request may name it.
    #[schema(value_type = Uuid)]
    pub id: OptionId,
    /// Its label, unique within the column ignoring case. A numeric
    /// select's labels are numbers.
    pub label: String,
}

/// A field that is `Some` whenever it is present, so `null` reads as
/// `Some(None)` and a missing one, by `default`, as `None`.
fn present<'de, Value, Input>(deserializer: Input) -> Result<Option<Value>, Input::Error>
where
    Value: Deserialize<'de>,
    Input: Deserializer<'de>,
{
    Value::deserialize(deserializer).map(Some)
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
    /// Rows of the table a relation column points at.
    #[schema(value_type = Vec<Uuid>)]
    Rows(Vec<RowId>),
    /// No value: the cell is emptied.
    Clear,
}

/// A select option, by its id or by its label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum OptionRef {
    /// An option the column has.
    #[schema(value_type = Uuid)]
    Id(OptionId),
    /// An option's label, matched without regard to case. An unknown label
    /// is refused.
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
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    utoipa::ToSchema,
    specta::Type,
    strum::IntoStaticStr,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
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

impl EntityKind {
    /// The kind as the properties system and the type names spell it.
    pub fn name(self) -> &'static str {
        self.into()
    }
}

/// What one op did, in the order the ops were sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OpResult {
    /// The table a creation added.
    #[serde(rename_all = "camelCase")]
    TableCreated {
        /// The new table.
        #[schema(value_type = Uuid)]
        table: TableId,
        /// Its version once the request committed.
        table_version: TableVersion,
    },
    /// A table's rename.
    #[serde(rename_all = "camelCase")]
    TableRenamed {
        /// The table's version once the request committed.
        table_version: TableVersion,
    },
    /// A table's removal.
    TableDeleted {
        /// The table removed.
        #[schema(value_type = Uuid)]
        table: TableId,
    },
    /// The database's tables in their new order.
    #[serde(rename_all = "camelCase")]
    TablesReordered {
        /// Every table, in its new order, with its version once the request
        /// committed.
        tables: Vec<VersionedTable>,
    },
    /// The column a creation added.
    #[serde(rename_all = "camelCase")]
    ColumnCreated {
        /// The new column.
        #[schema(value_type = Uuid)]
        column: ColumnId,
        /// The table's version once the request committed.
        table_version: TableVersion,
    },
    /// A column's rename.
    #[serde(rename_all = "camelCase")]
    ColumnRenamed {
        /// The table's version once the request committed.
        table_version: TableVersion,
    },
    /// A column's removal.
    #[serde(rename_all = "camelCase")]
    ColumnDeleted {
        /// The table's version once the request committed.
        table_version: TableVersion,
    },
    /// A table's columns in their new order.
    #[serde(rename_all = "camelCase")]
    ColumnsReordered {
        /// The table's version once the request committed.
        table_version: TableVersion,
    },
    /// The options an addition created.
    #[serde(rename_all = "camelCase")]
    OptionsAdded {
        /// The table's version once the request committed.
        table_version: TableVersion,
        /// The options created, in order: those sent, less any whose label
        /// the column already had.
        #[schema(value_type = Vec<Uuid>)]
        added: Vec<OptionId>,
    },
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
    },
    /// What an option change or removal did.
    #[serde(rename_all = "camelCase")]
    OptionChanged {
        /// The table's version after the change.
        table_version: TableVersion,
    },
    /// The view a creation or change left.
    #[serde(rename_all = "camelCase")]
    ViewWritten {
        /// The table's version after the change.
        table_version: TableVersion,
        /// The view as stored.
        view: Box<DatabaseView>,
    },
    /// A view's removal.
    #[serde(rename_all = "camelCase")]
    ViewDeleted {
        /// The table's version after the change.
        table_version: TableVersion,
    },
    /// The table's views' new places.
    #[serde(rename_all = "camelCase")]
    ViewsReordered {
        /// The table's version after the change.
        table_version: TableVersion,
        /// Every view's key, in their new order.
        positions: Vec<ViewPosition>,
    },
    /// Where a moved card, and any card it needed placed first, now sit.
    #[serde(rename_all = "camelCase")]
    CardMoved {
        /// The table's version after the change.
        table_version: TableVersion,
        /// The positions written, the moved card's last.
        positions: Vec<CardPosition>,
    },
}

/// An id a request minted for something new that already names something,
/// which refuses the request: a retried request whose first attempt
/// committed, or an id minted twice.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema, specta::Type,
)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum TakenId {
    /// A table's.
    #[schema(value_type = Uuid)]
    Table(TableId),
    /// A column's.
    #[schema(value_type = Uuid)]
    Column(ColumnId),
    /// An option's.
    #[schema(value_type = Uuid)]
    Option(OptionId),
}

/// A table and its version.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema, specta::Type,
)]
pub struct VersionedTable {
    /// The table.
    #[schema(value_type = Uuid)]
    pub table: TableId,
    /// Its version.
    pub version: TableVersion,
}

impl OpResult {
    /// The version of the op's table once the request committed; `None`
    /// when the op removed it, or names every table of the database.
    pub fn table_version(&self) -> Option<TableVersion> {
        match self {
            OpResult::TableDeleted { .. } | OpResult::TablesReordered { .. } => None,
            OpResult::TableCreated { table_version, .. }
            | OpResult::TableRenamed { table_version }
            | OpResult::ColumnCreated { table_version, .. }
            | OpResult::ColumnRenamed { table_version }
            | OpResult::ColumnDeleted { table_version }
            | OpResult::ColumnsReordered { table_version }
            | OpResult::OptionsAdded { table_version, .. }
            | OpResult::RowsWritten { table_version, .. }
            | OpResult::ColumnTyped { table_version }
            | OpResult::OptionChanged { table_version }
            | OpResult::ViewWritten { table_version, .. }
            | OpResult::ViewDeleted { table_version }
            | OpResult::ViewsReordered { table_version, .. }
            | OpResult::CardMoved { table_version, .. } => Some(*table_version),
        }
    }
}
