//! The fake cell store's write batch, applied straight to the world.

use super::*;

/// A first value settles the columns it landed in, as the cell store does in
/// its transaction.
pub(super) fn settle(world: &mut World, table_id: TableId, definitions: Vec<PropertyDefinitionId>) {
    if definitions.is_empty() {
        return;
    }
    for column in &mut world.columns {
        if column.table_id == table_id && definitions.contains(&column.property_definition_id) {
            column.infer_type = false;
        }
    }
    world.settled.push((table_id, definitions));
}

/// The fake cell store's batch, applied straight to the world; the caller
/// rolls the world back unless everything applied. An option for a missing
/// definition fails the batch, as the foreign key does in Postgres.
pub(super) fn apply_in_world(
    world: &mut World,
    writes: &Writes,
) -> Result<WritesOutcome, FakeError> {
    for table_id in writes
        .writes
        .iter()
        .flat_map(|write| write.versioned_tables().iter().copied())
    {
        let live = world
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .is_some_and(|table| {
                world.databases.iter().any(|database| {
                    database.id == table.database_id && database.trashed_at.is_none()
                })
            });
        if !live || world.table_write_not_found {
            return Ok(WritesOutcome::TableNotFound(table_id));
        }
    }
    for option in &writes.options {
        let definition = world
            .definitions
            .get_mut(&option.definition_id)
            .ok_or(FakeError)?;
        let display_order = definition.property_options.len() as i32;
        definition.property_options.push(PropertyOption {
            id: option.id.into_uuid(),
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
                    let id = RowId::from_uuid(Uuid::now_v7());
                    let table_rows = world.rows.entry(*table_id).or_default();
                    let position =
                        key_between(table_rows.last().map(|last| &last.position), None).unwrap();
                    table_rows.push(RowRef { id, position });
                    ids.push(id);
                    if !cells.is_empty() {
                        world
                            .cells
                            .entry(id)
                            .or_default()
                            .extend(cells.iter().cloned());
                    }
                    let valued: Vec<_> = cells.iter().map(|(definition, _)| *definition).collect();
                    settle(world, *table_id, valued);
                }
                inserted.push(ids);
            }
            Write::UpdateRows { table_id, rows } => {
                for (row, cells) in rows {
                    let owned = world
                        .rows
                        .get(table_id)
                        .is_some_and(|rows| rows.iter().any(|stored| stored.id == *row));
                    if !owned {
                        return Ok(WritesOutcome::MissingRow {
                            write: index,
                            row: *row,
                        });
                    }
                    let stored = world.cells.entry(*row).or_default();
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
                    settle(world, *table_id, valued);
                }
                inserted.push(Vec::new());
            }
            Write::DeleteRows { table_id, rows } => {
                for row in rows {
                    let table_rows = world.rows.entry(*table_id).or_default();
                    let Some(position) = table_rows.iter().position(|stored| stored.id == *row)
                    else {
                        return Ok(WritesOutcome::MissingRow {
                            write: index,
                            row: *row,
                        });
                    };
                    table_rows.remove(position);
                    world.cells.remove(row);
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
                let Some(option) =
                    world
                        .definitions
                        .get_mut(definition_id)
                        .and_then(|definition| {
                            definition
                                .property_options
                                .iter_mut()
                                .find(|option| option.id == option_id.into_uuid())
                        })
                else {
                    return Ok(WritesOutcome::MissingOption { write: index });
                };
                if let Some(value) = value {
                    option.value = value.clone();
                }
                if let Some(color) = color {
                    option.color.clone_from(color);
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
                if !rewrite_views(world, views) {
                    return Ok(WritesOutcome::MissingView { write: index });
                }
                let boards: Vec<ViewId> = world
                    .views
                    .iter()
                    .filter(|view| tables.contains(&view.table_id))
                    .map(|view| view.id)
                    .collect();
                for board in boards {
                    if let Some(placed) = world.positions.get_mut(&board) {
                        placed.retain(|card| card.lane != Some(*option_id));
                    }
                }
                let Some(definition) = world.definitions.get_mut(definition_id) else {
                    return Ok(WritesOutcome::MissingOption { write: index });
                };
                let before = definition.property_options.len();
                definition
                    .property_options
                    .retain(|option| option.id != option_id.into_uuid());
                if definition.property_options.len() == before {
                    return Ok(WritesOutcome::MissingOption { write: index });
                }
                for cells in world.cells.values_mut() {
                    if let Some(PropertyValue::SelectOption(options)) = cells.get_mut(definition_id)
                    {
                        options.retain(|option| *option != option_id.into_uuid());
                        if options.is_empty() {
                            cells.remove(definition_id);
                        }
                    }
                }
                inserted.push(Vec::new());
            }
            Write::CreateView { view } => {
                if world.views.iter().any(|other| {
                    other.table_id == view.table_id && other.name.eq_ignore_ascii_case(&view.name)
                }) {
                    return Ok(WritesOutcome::ViewNameTaken { write: index });
                }
                world.views.push(view.clone());
                inserted.push(Vec::new());
            }
            Write::UpdateView { view, regrouped } => {
                if !rewrite_views(world, std::slice::from_ref(view)) {
                    return Ok(WritesOutcome::MissingView { write: index });
                }
                if *regrouped {
                    world.positions.remove(&view.id);
                }
                inserted.push(Vec::new());
            }
            Write::DeleteView { table_id, view_id } => {
                let before = world.views.len();
                world
                    .views
                    .retain(|view| !(view.id == *view_id && view.table_id == *table_id));
                if world.views.len() == before {
                    return Ok(WritesOutcome::MissingView { write: index });
                }
                world.positions.remove(view_id);
                inserted.push(Vec::new());
            }
            Write::OrderViews {
                table_id,
                positions,
            } => {
                for placed in positions {
                    let Some(view) = world
                        .views
                        .iter_mut()
                        .find(|view| view.id == placed.view && view.table_id == *table_id)
                    else {
                        return Ok(WritesOutcome::MissingView { write: index });
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
                let owned = world
                    .rows
                    .get(table_id)
                    .is_some_and(|rows| rows.iter().any(|stored| stored.id == *row));
                if !owned {
                    return Ok(WritesOutcome::MissingRow {
                        write: index,
                        row: *row,
                    });
                }
                let stored = world.cells.entry(*row).or_default();
                match value {
                    Some(value) => {
                        stored.insert(*definition, value.clone());
                    }
                    None => {
                        stored.remove(definition);
                    }
                }
                let placed = world.positions.entry(*view_id).or_default();
                for position in positions {
                    placed.retain(|card| card.row != position.row);
                    placed.push(position.clone());
                }
                inserted.push(Vec::new());
            }
        }
    }
    for (table_id, row) in &writes.related_rows {
        if !world
            .rows
            .get(table_id)
            .is_some_and(|rows| rows.iter().any(|stored| stored.id == *row))
        {
            return Ok(WritesOutcome::MissingRelatedRow(*row));
        }
    }
    let mut table_versions = HashMap::new();
    for write in writes.writes.iter().filter(|write| write.changes()) {
        for table_id in write.versioned_tables() {
            if table_versions.contains_key(table_id) {
                continue;
            }
            let table = world
                .tables
                .iter_mut()
                .find(|table| table.id == *table_id)
                .unwrap();
            table.version.0 += 1;
            table_versions.insert(*table_id, table.version);
        }
    }
    Ok(WritesOutcome::Applied {
        inserted,
        table_versions,
    })
}
