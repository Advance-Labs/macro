//! The fake cell store's write batch, applied straight to the world.

use super::*;

/// A first value settles the columns it landed in, as the cell store does in
/// its transaction.
pub(super) fn settle(w: &mut World, table_id: TableId, definitions: Vec<PropertyDefinitionId>) {
    if definitions.is_empty() {
        return;
    }
    for column in &mut w.columns {
        if column.table_id == table_id && definitions.contains(&column.property_definition_id) {
            column.infer_type = false;
        }
    }
    w.settled.push((table_id, definitions));
}

/// The fake cell store's batch, applied straight to the world; the caller
/// rolls the world back unless everything applied.
pub(super) fn apply_in_world(w: &mut World, writes: &Writes) -> WritesOutcome {
    for table_id in writes
        .writes
        .iter()
        .flat_map(|write| write.versioned_tables().iter().copied())
    {
        let live = w.tables.iter().find(|t| t.id == table_id).is_some_and(|t| {
            w.databases
                .iter()
                .any(|d| d.id == t.database_id && d.trashed_at.is_none())
        });
        if !live || w.table_write_not_found {
            return WritesOutcome::TableNotFound(table_id);
        }
    }
    for option in &writes.options {
        let Some(definition) = w.definitions.get_mut(&option.definition_id) else {
            return WritesOutcome::TableNotFound(Uuid::nil());
        };
        let display_order = definition.property_options.len() as i32;
        definition.property_options.push(PropertyOption {
            id: option.id,
            property_definition_id: option.definition_id,
            display_order,
            value: option.value.clone(),
            color: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        });
    }
    let mut inserted = Vec::new();
    for (index, write) in writes.writes.iter().enumerate() {
        match write {
            Write::InsertRows { table_id, rows } => {
                let mut ids = Vec::new();
                for cells in rows {
                    let id = Uuid::now_v7();
                    let table_rows = w.rows.entry(*table_id).or_default();
                    table_rows.push(RowRef {
                        id,
                        position: format!("{:04}", table_rows.len()),
                    });
                    ids.push(id);
                    if !cells.is_empty() {
                        w.cells.entry(id).or_default().extend(cells.iter().cloned());
                    }
                    let valued: Vec<_> = cells.iter().map(|(definition, _)| *definition).collect();
                    settle(w, *table_id, valued);
                }
                inserted.push(ids);
            }
            Write::UpdateRows { table_id, rows } => {
                for (row, cells) in rows {
                    let owned = w
                        .rows
                        .get(table_id)
                        .is_some_and(|rows| rows.iter().any(|r| r.id == *row));
                    if !owned {
                        return WritesOutcome::MissingRow {
                            write: index,
                            row: *row,
                        };
                    }
                    let stored = w.cells.entry(*row).or_default();
                    for (definition, value) in cells {
                        match value {
                            Some(value) => {
                                stored.insert(*definition, value.clone());
                            }
                            None => {
                                stored.remove(definition);
                            }
                        }
                    }
                    let valued: Vec<_> = cells
                        .iter()
                        .filter(|(_, value)| value.is_some())
                        .map(|(definition, _)| *definition)
                        .collect();
                    settle(w, *table_id, valued);
                }
                inserted.push(Vec::new());
            }
            Write::DeleteRows { table_id, rows } => {
                for row in rows {
                    let table_rows = w.rows.entry(*table_id).or_default();
                    let Some(position) = table_rows.iter().position(|r| r.id == *row) else {
                        return WritesOutcome::MissingRow {
                            write: index,
                            row: *row,
                        };
                    };
                    table_rows.remove(position);
                    w.cells.remove(row);
                }
                inserted.push(Vec::new());
            }
            Write::UpdateOption {
                definition_id,
                option_id,
                value,
                color,
                ..
            } => {
                let Some(option) = w.definitions.get_mut(definition_id).and_then(|definition| {
                    definition
                        .property_options
                        .iter_mut()
                        .find(|option| option.id == *option_id)
                }) else {
                    return WritesOutcome::MissingOption { write: index };
                };
                if let Some(value) = value {
                    option.value = value.clone();
                }
                if let Some(color) = color {
                    option.color = color.clone();
                }
                inserted.push(Vec::new());
            }
            Write::DeleteOption {
                tables,
                definition_id,
                option_id,
                views,
                ..
            } => {
                if !rewrite_views(w, views) {
                    return WritesOutcome::MissingView { write: index };
                }
                let boards: Vec<ViewId> = w
                    .views
                    .iter()
                    .filter(|view| tables.contains(&view.table_id))
                    .map(|view| view.id)
                    .collect();
                for board in boards {
                    if let Some(placed) = w.positions.get_mut(&board) {
                        placed.retain(|card| card.lane != Some(*option_id));
                    }
                }
                let Some(definition) = w.definitions.get_mut(definition_id) else {
                    return WritesOutcome::MissingOption { write: index };
                };
                let before = definition.property_options.len();
                definition
                    .property_options
                    .retain(|option| option.id != *option_id);
                if definition.property_options.len() == before {
                    return WritesOutcome::MissingOption { write: index };
                }
                for cells in w.cells.values_mut() {
                    if let Some(PropertyValue::SelectOption(options)) = cells.get_mut(definition_id)
                    {
                        options.retain(|option| option != option_id);
                        if options.is_empty() {
                            cells.remove(definition_id);
                        }
                    }
                }
                inserted.push(Vec::new());
            }
            Write::CreateView { view } => {
                if w.views.iter().any(|other| {
                    other.table_id == view.table_id && other.name.eq_ignore_ascii_case(&view.name)
                }) {
                    return WritesOutcome::ViewNameTaken { write: index };
                }
                w.views.push(view.clone());
                inserted.push(Vec::new());
            }
            Write::UpdateView { view, regrouped } => {
                if !rewrite_views(w, std::slice::from_ref(view)) {
                    return WritesOutcome::MissingView { write: index };
                }
                if *regrouped {
                    w.positions.remove(&view.id);
                }
                inserted.push(Vec::new());
            }
            Write::DeleteView { table_id, view_id } => {
                let before = w.views.len();
                w.views
                    .retain(|view| !(view.id == *view_id && view.table_id == *table_id));
                if w.views.len() == before {
                    return WritesOutcome::MissingView { write: index };
                }
                w.positions.remove(view_id);
                inserted.push(Vec::new());
            }
            Write::OrderViews {
                table_id,
                positions,
            } => {
                for placed in positions {
                    let Some(view) = w
                        .views
                        .iter_mut()
                        .find(|view| view.id == placed.view && view.table_id == *table_id)
                    else {
                        return WritesOutcome::MissingView { write: index };
                    };
                    view.position = placed.position.clone();
                }
                inserted.push(Vec::new());
            }
            Write::MoveCard {
                table_id,
                view_id,
                row,
                positions,
                cell: (definition, value),
            } => {
                let owned = w
                    .rows
                    .get(table_id)
                    .is_some_and(|rows| rows.iter().any(|r| r.id == *row));
                if !owned {
                    return WritesOutcome::MissingRow {
                        write: index,
                        row: *row,
                    };
                }
                let stored = w.cells.entry(*row).or_default();
                match value {
                    Some(value) => {
                        stored.insert(*definition, value.clone());
                    }
                    None => {
                        stored.remove(definition);
                    }
                }
                let placed = w.positions.entry(*view_id).or_default();
                for position in positions {
                    placed.retain(|card| card.row != position.row);
                    placed.push(position.clone());
                }
                inserted.push(Vec::new());
            }
        }
    }
    for (table_id, row) in &writes.related_rows {
        if !w
            .rows
            .get(table_id)
            .is_some_and(|rows| rows.iter().any(|r| r.id == *row))
        {
            return WritesOutcome::MissingRelatedRow(*row);
        }
    }
    let mut table_versions = HashMap::new();
    for write in writes.writes.iter().filter(|write| write.changes()) {
        for table_id in write.versioned_tables() {
            if table_versions.contains_key(table_id) {
                continue;
            }
            let table = w.tables.iter_mut().find(|t| t.id == *table_id).unwrap();
            table.version.0 += 1;
            table_versions.insert(*table_id, table.version);
        }
    }
    WritesOutcome::Applied {
        inserted,
        table_versions,
    }
}
