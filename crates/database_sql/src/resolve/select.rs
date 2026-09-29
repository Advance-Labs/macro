//! `SELECT`: the select list against `GROUP BY`, aggregates against column
//! kinds, `ORDER BY` against the select list.

use crate::catalog::{ColumnKind, Table};
use crate::parse::{Agg, AggFn, Item, OrderBy, OrderKey as ParsedOrderKey, Select};

use super::{Order, OrderKey, ResolveError, SelectItem, SelectQuery, filter, names};

pub fn resolve(table: &Table, select: Select) -> Result<SelectQuery, ResolveError> {
    let group_by = select
        .group_by
        .as_ref()
        .map(|name| names::column(table, name))
        .transpose()?;

    let items = if select.items == [Item::Star] {
        table
            .columns
            .iter()
            .map(|column| SelectItem::Column(column.id))
            .collect::<Vec<_>>()
    } else {
        select
            .items
            .iter()
            .map(|item| resolve_item(table, item))
            .collect::<Result<_, _>>()?
    };

    let aggregates = items
        .iter()
        .any(|item| matches!(item, SelectItem::Agg { .. }));
    if aggregates || group_by.is_some() {
        for item in &items {
            if let SelectItem::Column(id) = item
                && group_by.map(|column| column.id) != Some(*id)
            {
                return Err(ResolveError::ColumnNotGrouped {
                    column: column_name(table, *id),
                    grouped: group_by.is_some(),
                });
            }
        }
    }

    let where_ = select
        .where_
        .map(|cond| filter::resolve(table, cond))
        .transpose()?;

    let order_by = select
        .order_by
        .iter()
        .map(|order| resolve_order(table, &items, group_by.map(|column| column.id), order))
        .collect::<Result<_, _>>()?;

    Ok(SelectQuery {
        table: table.id,
        items,
        where_,
        group_by: group_by.map(|column| column.id),
        order_by,
    })
}

fn resolve_item(table: &Table, item: &Item) -> Result<SelectItem, ResolveError> {
    match item {
        Item::Star => unreachable!("a lone * is expanded before items are resolved"),
        Item::Column(name) => Ok(SelectItem::Column(names::column(table, name)?.id)),
        Item::Agg(agg) => resolve_agg(table, agg),
    }
}

fn resolve_agg(table: &Table, agg: &Agg) -> Result<SelectItem, ResolveError> {
    let Some(name) = &agg.arg else {
        return Ok(SelectItem::Agg {
            func: agg.func,
            column: None,
        });
    };
    let column = names::column(table, name)?;
    let allowed = match agg.func {
        AggFn::Count => true,
        AggFn::Sum | AggFn::Avg => column.kind == ColumnKind::Number,
        AggFn::Min | AggFn::Max => matches!(column.kind, ColumnKind::Number | ColumnKind::Date),
    };
    if !allowed {
        return Err(ResolveError::AggregateNotSupported {
            func: agg.func.name(),
            column: column.name.clone(),
            kind: column.kind.describe(),
        });
    }
    Ok(SelectItem::Agg {
        func: agg.func,
        column: Some(column.id),
    })
}

fn resolve_order(
    table: &Table,
    items: &[SelectItem],
    group_by: Option<uuid::Uuid>,
    order: &OrderBy,
) -> Result<Order, ResolveError> {
    let key = match &order.key {
        ParsedOrderKey::Position(position) => {
            let index = *position as usize - 1;
            if index >= items.len() {
                return Err(ResolveError::OrderPositionOutOfRange {
                    position: *position,
                    items: items.len(),
                });
            }
            OrderKey::Item(index)
        }
        ParsedOrderKey::Agg(agg) => {
            let wanted = resolve_agg(table, agg)?;
            items
                .iter()
                .position(|item| *item == wanted)
                .map(OrderKey::Item)
                .ok_or_else(|| ResolveError::OrderAggregateNotSelected {
                    agg: agg.to_string(),
                })?
        }
        ParsedOrderKey::Column(name) => {
            let column = names::column(table, name)?;
            let grouped = group_by.is_some()
                || items
                    .iter()
                    .any(|item| matches!(item, SelectItem::Agg { .. }));
            if grouped && group_by != Some(column.id) {
                return Err(ResolveError::OrderColumnNotGrouped {
                    column: column.name.clone(),
                });
            }
            OrderKey::Column(column.id)
        }
    };
    Ok(Order {
        key,
        dir: order.dir,
    })
}

fn column_name(table: &Table, id: uuid::Uuid) -> String {
    table
        .columns
        .iter()
        .find(|column| column.id == id)
        .map(|column| column.name.clone())
        .expect("select items are resolved from this table")
}

impl AggFn {
    /// The function as written.
    pub fn name(self) -> &'static str {
        match self {
            AggFn::Count => "COUNT",
            AggFn::Sum => "SUM",
            AggFn::Avg => "AVG",
            AggFn::Min => "MIN",
            AggFn::Max => "MAX",
        }
    }
}

impl std::fmt::Display for Agg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.arg {
            Some(column) => write!(f, "{}({})", self.func.name(), column.0),
            None => write!(f, "{}(*)", self.func.name()),
        }
    }
}
