//! Which parts of a `WHERE` the Soup `propf` expression can express.
//!
//! Soup matches a select option or an entity reference on a property, and
//! combines matches with and/or. It also has `not`, but Soup's `not` is a set
//! difference from every row, so it keeps rows whose cell is empty, while
//! SQL's `!=` drops them. Negated forms therefore never push down.
//!
//! A top-level `AND` pushes its pushable conjuncts and keeps the rest; an
//! `OR` pushes only when every side does, because a half-pushed `OR` would
//! drop rows the other side wanted.

use filter_ast::Expr;
use item_filters::ast::properties::{EntityRefId, PropertiesLiteral, PropertyMatchValue};

use crate::resolve::{CmpOp, Filter, Value};

/// The pushed-down expression and the filter that remains, either possibly
/// absent.
pub fn divide(filter: Filter) -> (Option<Expr<PropertiesLiteral>>, Option<Filter>) {
    match filter {
        Filter::And(parts) => {
            let mut pushed = Vec::new();
            let mut kept = Vec::new();
            for part in parts {
                match push(&part) {
                    Some(expr) => pushed.push(expr),
                    None => kept.push(part),
                }
            }
            let residual = match kept.len() {
                0 => None,
                1 => kept.pop(),
                _ => Some(Filter::And(kept)),
            };
            (pushed.into_iter().reduce(Expr::and), residual)
        }
        other => match push(&other) {
            Some(expr) => (Some(expr), None),
            None => (None, Some(other)),
        },
    }
}

/// The whole filter as a `propf` expression, or `None` if any part of it
/// cannot be expressed.
fn push(filter: &Filter) -> Option<Expr<PropertiesLiteral>> {
    match filter {
        Filter::Cmp {
            column,
            op: CmpOp::Eq,
            value,
        } => literal(*column, value),
        Filter::In {
            column,
            values,
            negated: false,
        } => values
            .iter()
            .map(|value| literal(*column, value))
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .reduce(Expr::or),
        Filter::Has {
            column,
            value,
            negated: false,
        } => literal(*column, value),
        Filter::And(parts) => parts
            .iter()
            .map(push)
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .reduce(Expr::and),
        Filter::Or(parts) => parts
            .iter()
            .map(push)
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .reduce(Expr::or),
        _ => None,
    }
}

/// A match on one option or one entity reference.
fn literal(column: uuid::Uuid, value: &Value) -> Option<Expr<PropertiesLiteral>> {
    let value = match value {
        Value::Option(option) => PropertyMatchValue::SelectOption(*option),
        // Resolve accepted the id; the ref type rejects only quotes and
        // backslashes, which no id contains.
        Value::Entity(id) => PropertyMatchValue::EntityRef(EntityRefId::new(id.clone()).ok()?),
        _ => return None,
    };
    Some(Expr::Literal(PropertiesLiteral {
        property_definition_id: column,
        entity_type: None,
        value,
    }))
}
