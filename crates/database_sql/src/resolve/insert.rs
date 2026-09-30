//! `INSERT` and `UPDATE`: columns resolved once, every value typed for its
//! column.

use crate::catalog::Table;
use uuid::Uuid;

use crate::parse::{Insert, Lit, Update};

use super::{InsertQuery, ResolveError, UpdateQuery, filter, names};

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
                .map(|(value, column)| Ok((column.id, filter::typed_cell(column, value)?)))
                .collect::<Result<Vec<_>, ResolveError>>()
        })
        .collect::<Result<_, _>>()?;

    Ok(InsertQuery {
        table: table.id,
        rows,
    })
}

pub fn resolve_update(table: &Table, update: Update) -> Result<UpdateQuery, ResolveError> {
    let mut cells: Vec<(Uuid, Option<super::Value>)> = Vec::with_capacity(update.assignments.len());
    for (name, value) in update.assignments {
        let column = names::column(table, &name)?;
        if cells.iter().any(|(seen, _)| *seen == column.id) {
            return Err(ResolveError::DuplicateInsertColumn {
                column: column.name.clone(),
            });
        }
        let value = match value {
            Lit::Null => None,
            value => Some(filter::typed_cell(column, value)?),
        };
        cells.push((column.id, value));
    }
    Ok(UpdateQuery {
        table: table.id,
        row_id: row_id(&update.row_id)?,
        cells,
    })
}

/// The row id a write names, which must be a UUID.
pub fn row_id(written: &str) -> Result<Uuid, ResolveError> {
    Uuid::parse_str(written).map_err(|_| ResolveError::RowIdNotAnId {
        written: written.to_owned(),
    })
}
