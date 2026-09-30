//! Which column type changes keep a column's values: one rule, read as a
//! table, that the server's type change, its dry run, the agent tools and
//! `ALTER COLUMN … TYPE` all consult before touching data.

#[cfg(test)]
mod test;

use std::fmt;

use crate::catalog::{ColumnKind, EntityKind};

/// Whether a column has any values. Emptiness is a fact about the data, not
/// the type: an empty column can take any type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Contents {
    /// No cell has a value.
    Empty,
    /// At least one cell has a value.
    Filled,
}

/// What changing a column from one type to another does to its values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cast {
    /// Every value converts.
    Safe,
    /// Some values may not convert; each must be checked first.
    Checked,
    /// No value converts; why, in one line.
    Never(&'static str),
}

/// A type a column can be changed to, spelled in SQL the way
/// `DescribeDatabase` names column types: `text`, `select[]`,
/// `entity(USER)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType {
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
}

/// The types a column is offered to change to, in menu order.
pub const TARGETS: [ColumnType; 10] = [
    ColumnType::Text,
    ColumnType::Number,
    ColumnType::Select { multi: false },
    ColumnType::Select { multi: true },
    ColumnType::Date,
    ColumnType::Boolean,
    ColumnType::Link,
    ColumnType::Entity {
        target: EntityKind::User,
        multi: false,
    },
    ColumnType::Entity {
        target: EntityKind::Document,
        multi: false,
    },
    ColumnType::Entity {
        target: EntityKind::Task,
        multi: false,
    },
];

impl ColumnType {
    /// The kind a column of this type has, without options.
    pub fn kind(&self) -> ColumnKind {
        match *self {
            ColumnType::Text => ColumnKind::Text,
            ColumnType::Number => ColumnKind::Number,
            ColumnType::Boolean => ColumnKind::Boolean,
            ColumnType::Date => ColumnKind::Date,
            ColumnType::Link => ColumnKind::Link,
            ColumnType::Select { multi } | ColumnType::SelectNumber { multi } => {
                ColumnKind::Select {
                    multi,
                    options: Vec::new(),
                }
            }
            ColumnType::Tag => ColumnKind::Select {
                multi: true,
                options: Vec::new(),
            },
            ColumnType::Entity { target, multi } => ColumnKind::Entity { multi, target },
        }
    }
}

impl fmt::Display for ColumnType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let several = |multi: bool| if multi { "[]" } else { "" };
        match self {
            ColumnType::Text => f.write_str("text"),
            ColumnType::Number => f.write_str("number"),
            ColumnType::Boolean => f.write_str("boolean"),
            ColumnType::Date => f.write_str("date"),
            ColumnType::Link => f.write_str("link"),
            ColumnType::Select { multi } => write!(f, "select{}", several(*multi)),
            ColumnType::SelectNumber { multi } => write!(f, "select_number{}", several(*multi)),
            ColumnType::Tag => f.write_str("tag"),
            ColumnType::Entity { target, multi } => {
                write!(f, "entity({}){}", target.sql_name(), several(*multi))
            }
        }
    }
}

const TO_REFERENCES: &str = "Only an empty column can become a reference column.";
const TO_RELATION: &str =
    "Only an empty column can become a relation: existing values aren't rows.";
const FROM_RELATION: &str = "A relation's linked rows can't be converted; remove them first.";
const ACROSS_ENTITIES: &str = "References can't change what they point at.";
const ENTITY_TO_VALUE: &str = "References can't become plain values.";
const NUMBER_TO_DATE: &str = "Numbers aren't dates.";
const NUMBER_TO_CHECKBOX: &str = "Numbers aren't checkboxes.";
const NUMBER_TO_URL: &str = "Numbers aren't URLs.";
const FROM_CHECKBOX: &str = "A checkbox can only become text.";
const FROM_DATE: &str = "A date can only become text.";
const FROM_URL: &str = "A URL can only become text or a select.";

/// What changing a column of kind `from` to kind `to` does to its values.
/// Select options play no part: only the kinds and whether they hold
/// several values do.
pub fn cast(from: &ColumnKind, to: &ColumnKind, contents: Contents) -> Cast {
    use Cast::{Checked, Never, Safe};
    use ColumnKind::{Boolean, Date, Entity, Link, Number, Select, Text};
    use EntityKind::Row;

    match (from, to) {
        _ if contents == Contents::Empty => Safe,

        (Entity { target: Row, .. }, _) => Never(FROM_RELATION),
        (_, Entity { target: Row, .. }) => Never(TO_RELATION),
        (Entity { target: from, .. }, Entity { target: to, .. }) if from != to => {
            Never(ACROSS_ENTITIES)
        }
        (Entity { multi: true, .. }, Entity { multi: false, .. }) => Checked,
        (Entity { .. }, Entity { .. }) => Safe,
        (Entity { .. }, _) => Never(ENTITY_TO_VALUE),
        (_, Entity { .. }) => Never(TO_REFERENCES),

        (Text, Text) => Safe,
        (Text, Number | Date | Boolean | Link | Select { .. }) => Checked,

        (Number, Text | Number | Select { .. }) => Safe,
        (Number, Date) => Never(NUMBER_TO_DATE),
        (Number, Boolean) => Never(NUMBER_TO_CHECKBOX),
        (Number, Link) => Never(NUMBER_TO_URL),

        (Boolean, Text | Boolean) => Safe,
        (Boolean, _) => Never(FROM_CHECKBOX),

        (Date, Text | Date) => Safe,
        (Date, _) => Never(FROM_DATE),

        (Select { multi: false, .. }, Text | Select { .. }) => Safe,
        (Select { multi: false, .. }, Number | Link | Date | Boolean) => Checked,

        (Select { multi: true, .. }, Select { multi: true, .. }) => Safe,
        (Select { multi: true, .. }, Text | Select { .. } | Number | Link | Date | Boolean) => {
            Checked
        }

        (Link, Text | Link) => Safe,
        (Link, Select { multi: false, .. }) => Checked,
        (Link, _) => Never(FROM_URL),
    }
}
