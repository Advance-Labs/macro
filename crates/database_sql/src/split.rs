//! Stage three: divide a resolved `SELECT` into what the server answers and
//! what we finish ourselves.
//!
//! The server side is a Soup query: a table scope plus, when the filter
//! allows it, a `propf` expression the server evaluates with its indexes.
//! Everything Soup cannot express — comparisons on numbers, dates, text and
//! checkboxes, negations (whose `NULL` semantics differ from SQL's), sorting
//! by a column, aggregation — is left in the [`Plan`] for the fold to apply
//! to the rows that come back.
//!
//! Split is total: every resolved query has a plan.

mod pushdown;
#[cfg(test)]
mod test;

use filter_ast::Expr;
use item_filters::ast::properties::PropertiesLiteral;
use uuid::Uuid;

use crate::catalog::{Catalog, ColumnKind};
use crate::resolve::{AggFn, Filter, Order, OrderKey, SelectItem, SelectQuery};

/// A resolved `SELECT`, divided.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// What to ask the server.
    pub gql: GqlQuery,
    /// The columns whose values the fetched rows must carry for the rest of
    /// the plan to run: selected, aggregated, grouped, sorted on, or tested by
    /// the residual filter. Pushed-down conditions need nothing back.
    pub needs: Vec<Uuid>,
    /// The part of `WHERE` the server did not apply.
    pub residual: Option<Filter>,
    /// The shape of the result: plain rows or aggregates.
    pub shape: Shape,
    /// The ordering, applied after `shape`.
    pub order_by: Vec<Order>,
}

/// Every GraphQL query a plan can send.
#[derive(Debug, Clone, PartialEq)]
pub enum GqlQuery {
    /// `Query.soup` scoped to one table, paged to completion.
    Soup {
        /// The table whose rows are read.
        table: Uuid,
        /// The pushed-down part of `WHERE`, as the Soup `propf` expression.
        propf: Option<Expr<PropertiesLiteral>>,
    },
    /// `Query.groupSoup` scoped to one table, for `COUNT(*)` per group of a
    /// select or entity column: the bins' `totalCount` answers the query
    /// without fetching rows.
    GroupSoup {
        /// The table whose rows are counted.
        table: Uuid,
        /// The pushed-down part of `WHERE`.
        propf: Option<Expr<PropertiesLiteral>>,
        /// The column whose values form the bins.
        group_by: Uuid,
    },
}

/// What the result rows look like.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    /// One result row per fetched row, with these columns.
    Rows(Vec<Uuid>),
    /// One result row per group (or one row in all, without `GROUP BY`),
    /// with these items.
    Aggregate {
        /// The column grouped on.
        group_by: Option<Uuid>,
        /// The select list, in order.
        items: Vec<SelectItem>,
    },
}

/// Divide a resolved `SELECT`.
pub fn split(catalog: &Catalog, query: SelectQuery) -> Plan {
    let (propf, residual) = match query.where_ {
        Some(filter) => pushdown::divide(filter),
        None => (None, None),
    };

    let aggregates = query
        .items
        .iter()
        .any(|item| matches!(item, SelectItem::Agg { .. }));
    let shape = if aggregates || query.group_by.is_some() {
        Shape::Aggregate {
            group_by: query.group_by,
            items: query.items.clone(),
        }
    } else {
        Shape::Rows(
            query
                .items
                .iter()
                .map(|item| match item {
                    SelectItem::Column(id) => *id,
                    SelectItem::Agg { .. } => unreachable!("no aggregates in a row shape"),
                })
                .collect(),
        )
    };

    let counts_bins = residual.is_none()
        && query.group_by.is_some_and(|group| {
            groups_server_side(catalog, group)
                && query.items.iter().all(|item| {
                    *item == SelectItem::Column(group)
                        || matches!(
                            item,
                            SelectItem::Agg {
                                func: AggFn::Count,
                                column: None
                            }
                        )
                })
        });

    let gql = match (counts_bins, query.group_by) {
        (true, Some(group_by)) => GqlQuery::GroupSoup {
            table: query.table,
            propf,
            group_by,
        },
        _ => GqlQuery::Soup {
            table: query.table,
            propf,
        },
    };

    let needs = if counts_bins {
        Vec::new()
    } else {
        needed_columns(
            &query.items,
            query.group_by,
            &query.order_by,
            residual.as_ref(),
        )
    };

    Plan {
        gql,
        needs,
        residual,
        shape,
        order_by: query.order_by,
    }
}

/// Whether `groupSoup` can bin on the column: only select and entity values
/// are indexed as facts.
fn groups_server_side(catalog: &Catalog, column: Uuid) -> bool {
    catalog
        .tables
        .iter()
        .flat_map(|table| &table.columns)
        .find(|candidate| candidate.id == column)
        .is_some_and(|column| {
            matches!(
                column.kind,
                ColumnKind::Select { .. } | ColumnKind::Entity { .. }
            )
        })
}

/// Every column the fold reads, first use first, no repeats.
fn needed_columns(
    items: &[SelectItem],
    group_by: Option<Uuid>,
    order_by: &[Order],
    residual: Option<&Filter>,
) -> Vec<Uuid> {
    let mut needs = Vec::new();
    let mut need = |column: Uuid| {
        if !needs.contains(&column) {
            needs.push(column);
        }
    };
    for item in items {
        match item {
            SelectItem::Column(column) => need(*column),
            SelectItem::Agg {
                column: Some(column),
                ..
            } => need(*column),
            SelectItem::Agg { column: None, .. } => {}
        }
    }
    if let Some(column) = group_by {
        need(column);
    }
    for order in order_by {
        if let OrderKey::Column(column) = order.key {
            need(column);
        }
    }
    if let Some(filter) = residual {
        filter.for_each_column(&mut need);
    }
    needs
}

impl Filter {
    /// Visit every column the filter tests, in source order.
    pub fn for_each_column(&self, visit: &mut impl FnMut(Uuid)) {
        match self {
            Filter::Cmp { column, .. }
            | Filter::In { column, .. }
            | Filter::Has { column, .. }
            | Filter::IsNull { column, .. }
            | Filter::Like { column, .. } => visit(*column),
            Filter::And(parts) | Filter::Or(parts) => {
                for part in parts {
                    part.for_each_column(visit);
                }
            }
        }
    }
}
