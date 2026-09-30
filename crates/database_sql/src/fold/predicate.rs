//! Evaluating the residual filter on one row.

use std::cmp::Ordering;

use crate::resolve::{CmpOp, Filter, Value};

use super::{Cell, Row};

/// Whether the row satisfies the filter. An empty cell fails every
/// comparison, as `NULL` does in SQL; `IS NULL` is the only test it passes.
pub fn holds(filter: &Filter, row: &Row) -> bool {
    match filter {
        Filter::And(parts) => parts.iter().all(|part| holds(part, row)),
        Filter::Or(parts) => parts.iter().any(|part| holds(part, row)),
        Filter::IsNull { column, negated } => {
            let empty = match row.cells.get(column) {
                None => true,
                Some(Cell::Options(ids)) => ids.is_empty(),
                Some(Cell::Entities(ids)) => ids.is_empty(),
                Some(_) => false,
            };
            empty != *negated
        }
        Filter::Cmp { column, op, value } => row
            .cells
            .get(column)
            .and_then(|cell| compare(cell, value))
            .is_some_and(|ordering| match op {
                CmpOp::Eq => ordering == Ordering::Equal,
                CmpOp::Ne => ordering != Ordering::Equal,
                CmpOp::Lt => ordering == Ordering::Less,
                CmpOp::Le => ordering != Ordering::Greater,
                CmpOp::Gt => ordering == Ordering::Greater,
                CmpOp::Ge => ordering != Ordering::Less,
            }),
        Filter::In {
            column,
            values,
            negated,
        } => row.cells.get(column).is_some_and(|cell| {
            let listed = values
                .iter()
                .any(|value| compare(cell, value) == Some(Ordering::Equal));
            listed != *negated
        }),
        Filter::Has {
            column,
            value,
            negated,
        } => {
            let has = match (row.cells.get(column), value) {
                (Some(Cell::Options(ids)), Value::Option(id)) => ids.contains(id),
                (Some(Cell::Entities(ids)), Value::Entity(id)) => ids.contains(id),
                _ => false,
            };
            has != *negated
        }
        Filter::Like {
            column,
            pattern,
            escape,
            negated,
        } => match row.cells.get(column) {
            Some(Cell::Text(text)) => like(pattern, *escape, text) != *negated,
            _ => false,
        },
    }
}

/// Order a cell against a typed value; `None` when they are not comparable,
/// which resolve rules out except for empty multi-valued cells.
fn compare(cell: &Cell, value: &Value) -> Option<Ordering> {
    match (cell, value) {
        (Cell::Text(a), Value::Text(b)) => Some(a.as_str().cmp(b.as_str())),
        (Cell::Number(a), Value::Number(b)) => a.partial_cmp(b),
        (Cell::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
        (Cell::Date(a), Value::Date(b)) => Some(a.cmp(b)),
        (Cell::Options(ids), Value::Option(id)) => single(ids).map(|only| only.cmp(id)),
        (Cell::Entities(ids), Value::Entity(id)) => {
            single(ids).map(|only| only.as_str().cmp(id.as_str()))
        }
        _ => None,
    }
}

/// The one value of a single-valued cell; an empty cell has none.
fn single<T>(ids: &[T]) -> Option<&T> {
    match ids {
        [only] => Some(only),
        _ => None,
    }
}

/// SQL `LIKE` with `%` and `_`, ignoring case. The escape character makes
/// the character after it literal.
fn like(pattern: &str, escape: Option<char>, text: &str) -> bool {
    let escape = escape.map(lowercase);
    let mut parts = Vec::new();
    let lowered = pattern.to_lowercase();
    let mut characters = lowered.chars();
    while let Some(character) = characters.next() {
        parts.push(match character {
            _ if Some(character) == escape => {
                // Parse rejects a pattern that ends with its escape.
                Part::Literal(characters.next().unwrap_or(character))
            }
            '%' => Part::Any,
            '_' => Part::One,
            literal => Part::Literal(literal),
        });
    }
    let text: Vec<char> = text.to_lowercase().chars().collect();
    matches(&parts, &text)
}

/// A character's lower case when it is one character, as `to_lowercase` of the
/// whole pattern would spell it.
fn lowercase(character: char) -> char {
    let mut lower = character.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(only), None) => only,
        _ => character,
    }
}

/// One element of a `LIKE` pattern.
enum Part {
    /// `%`: any run of characters.
    Any,
    /// `_`: exactly one character.
    One,
    /// This character.
    Literal(char),
}

fn matches(pattern: &[Part], text: &[char]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some((Part::Any, rest)) => (0..=text.len()).any(|skip| matches(rest, &text[skip..])),
        Some((Part::One, rest)) => !text.is_empty() && matches(rest, &text[1..]),
        Some((Part::Literal(literal), rest)) => {
            text.first() == Some(literal) && matches(rest, &text[1..])
        }
    }
}
