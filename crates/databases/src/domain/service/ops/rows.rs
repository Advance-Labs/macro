//! The row ops: inserting, updating and deleting a table's rows, their cells
//! checked against the columns as earlier ops leave them.

use models_databases::{RowChanges, RowsChange};

use super::{MAX_WRITTEN_ROWS, Planner, refuse};
use crate::domain::catalog::TableEntry;
use crate::domain::models::{DatabaseError, Write};

impl Planner {
    pub(super) fn rows_write(
        &mut self,
        index: usize,
        entry: &TableEntry,
        change: &RowsChange,
    ) -> Result<Write, DatabaseError> {
        let table = entry.table.id;
        self.written_rows += match change {
            RowsChange::Insert { rows } => rows.len(),
            RowsChange::Update {
                changes: RowChanges::Uniform { rows, .. },
            }
            | RowsChange::Delete { rows } => rows.len(),
            RowsChange::Update {
                changes: RowChanges::PerRow { rows },
            } => rows.len(),
        };
        if self.written_rows > MAX_WRITTEN_ROWS {
            return Err(refuse(
                index,
                None,
                None,
                format!("a request writes at most {MAX_WRITTEN_ROWS} rows"),
            ));
        }
        self.written_tables.insert(table);
        match change {
            RowsChange::Insert { rows } => {
                let rows = rows
                    .iter()
                    .enumerate()
                    .map(|(row, cells)| {
                        let cells = self.cells(entry, index, Some(row), cells)?;
                        Ok(cells
                            .into_iter()
                            .filter_map(|(definition, value)| {
                                value.map(|value| (definition, value))
                            })
                            .collect())
                    })
                    .collect::<Result<_, DatabaseError>>()?;
                Ok(Write::InsertRows {
                    table_id: table,
                    rows,
                })
            }
            RowsChange::Update {
                changes: RowChanges::Uniform { rows, cells },
            } => {
                let cells = self.cells(entry, index, None, cells)?;
                Ok(Write::UpdateRows {
                    table_id: table,
                    rows: rows.iter().map(|row| (*row, cells.clone())).collect(),
                })
            }
            RowsChange::Update {
                changes: RowChanges::PerRow { rows },
            } => Ok(Write::UpdateRows {
                table_id: table,
                rows: rows
                    .iter()
                    .enumerate()
                    .map(|(row, change)| {
                        let cells = self.cells(entry, index, Some(row), &change.cells)?;
                        Ok((change.row, cells))
                    })
                    .collect::<Result<_, DatabaseError>>()?,
            }),
            RowsChange::Delete { rows } => {
                for (row, id) in rows.iter().enumerate() {
                    if rows[..row].contains(id) {
                        return Err(refuse(
                            index,
                            Some(row),
                            None,
                            format!("row {id} is named twice"),
                        ));
                    }
                }
                Ok(Write::DeleteRows {
                    table_id: table,
                    rows: rows.clone(),
                })
            }
        }
    }
}
