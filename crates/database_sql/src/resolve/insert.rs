//! `INSERT`: columns resolved once, every value typed for its column.

use crate::catalog::Table;
use crate::parse::{Insert, Lit};

use super::{InsertQuery, ResolveError, filter, names};

pub fn resolve(table: &Table, insert: Insert) -> Result<InsertQuery, ResolveError> {
    let mut columns = Vec::with_capacity(insert.columns.len());
    for name in &insert.columns {
        let column = names::column(table, name)?;
        if columns
            .iter()
            .any(|seen: &&crate::catalog::Column| seen.id == column.id)
        {
            return Err(ResolveError::DuplicateInsertColumn {
                column: column.name.clone(),
            });
        }
        columns.push(column);
    }

    let rows = insert
        .rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .zip(&columns)
                .filter(|(value, _)| *value != Lit::Null)
                .map(|(value, column)| Ok((column.id, filter::typed(column, value)?)))
                .collect::<Result<Vec<_>, ResolveError>>()
        })
        .collect::<Result<_, _>>()?;

    Ok(InsertQuery {
        table: table.id,
        rows,
    })
}
