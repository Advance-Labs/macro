use super::column_types::{ConvertedCell, Converter, is_empty};
use super::views::{views_without_column, views_without_tests_of};
use super::*;
use crate::domain::catalog::{ColumnEntry, PropertyType};
use models_databases::cast::{Cast, Contents, cast};
use models_databases::views::written_at;
use models_properties::service::property_value::PropertyValue;

impl<Repository, Definitions, Cells, Events, Access, Broker>
    DatabasesServiceImpl<Repository, Definitions, Cells, Events, Access, Broker>
where
    Repository: DatabasesRepo,
    Definitions: ColumnDefinitionStore,
    Cells: CellStore,
    Events: TableEventPublisher,
    Access: AccessDirectory,
    Broker: MacroEventBroker,
{
    pub(super) async fn change_placement_type(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        viewer: Viewer,
        command: ChangeColumnType,
    ) -> Result<ColumnTypeChangeOutcome, DatabaseError> {
        let database_id = receipt_database_id(&receipt)?;
        self.retype_column(database_id, receipt_attribution(&receipt), &viewer, command)
            .await
    }

    /// Change a column's type, for a caller already known to hold edit
    /// access to `database_id`: the type menu and the agent tool through a
    /// receipt, `ALTER COLUMN` through its op. The cast rule is consulted
    /// before any cell is read for conversion.
    pub(super) async fn retype_column(
        &self,
        database_id: DatabaseId,
        attribution: Option<events::Attribution>,
        viewer: &Viewer,
        command: ChangeColumnType,
    ) -> Result<ColumnTypeChangeOutcome, DatabaseError> {
        let (database, tables) = self
            .repository
            .get_database(database_id)
            .await
            .map_err(repository_error)?
            .filter(|(database, _)| database.trashed_at.is_none())
            .ok_or(DatabaseError::NotFound)?;
        let table = tables
            .iter()
            .find(|table| table.id == command.table_id)
            .ok_or(DatabaseError::NotFound)?;
        if table.version != command.base_version {
            return Err(DatabaseError::VersionConflict);
        }
        if command.relation.is_some() && command.data_type != DataType::Entity {
            return Err(DatabaseError::from(SchemaError::RelationNotEntity));
        }
        let command = ChangeColumnType {
            is_multi_select: command.is_multi_valued(),
            ..command
        };
        if (command.specific_entity_type.is_some()
            && (command.data_type != DataType::Entity || command.relation.is_some()))
            || (command.data_type == DataType::Entity
                && command.relation.is_none()
                && command.specific_entity_type.is_none())
            || (command.is_multi_select
                && !matches!(
                    command.data_type,
                    DataType::SelectString
                        | DataType::SelectNumber
                        | DataType::Tag
                        | DataType::Entity
                        | DataType::Link
                ))
        {
            return Err(DatabaseError::from(SchemaError::UnsupportedColumnType));
        }
        if let Some((database_id, table_id)) = command.relation
            && (self
                .live_database_grant(viewer, database_id)
                .await
                .map_err(DatabaseError::Repo)?
                .is_none()
                || !self
                    .repository
                    .tables_for_databases(&[database_id])
                    .await
                    .map_err(repository_error)?
                    .iter()
                    .any(|table| table.id == table_id))
        {
            return Err(DatabaseError::from(SchemaError::RelatedTableInaccessible));
        }
        let detail = self
            .column_detail(database.id, AccessLevel::Edit, table.id, command.column_id)
            .await?;
        if let Some(blocker) = self.retype_blocker(table.id, &detail).await? {
            return Err(blocker.into());
        }
        let current = PropertyType::of(&detail.column, &detail.definition);
        let target = PropertyType {
            data_type: command.data_type,
            is_multi_select: command.is_multi_select,
            specific_entity_type: command.specific_entity_type,
            relation: command.relation.is_some(),
        };
        let same_relation = match (&detail.column.config, command.relation) {
            (Some(ColumnConfig::Link { table_id, .. }), Some((_, target_table))) => {
                *table_id == target_table
            }
            (_, None) => true,
            _ => false,
        };
        if current == target && same_relation {
            return Ok(ColumnTypeChangeOutcome {
                table_versions: HashMap::from([(table.id, table.version)]),
                cleared_cells: 0,
                trimmed_cells: 0,
            });
        }

        let rows = self.rows_with_cells(table.id).await?;
        if rows.len() > MAX_CONVERTED_ROWS {
            return Err(DatabaseError::from(SchemaError::TooManyRowsToRetype));
        }
        let definition_id = detail.definition.definition.id;
        let values = rows.iter().filter_map(|(row, cells)| {
            cells
                .get(&definition_id)
                .filter(|value| !is_empty(value))
                .map(|value| (row.id, value))
        });
        let contents = if values.clone().next().is_some() {
            Contents::Filled
        } else {
            Contents::Empty
        };
        let entry = ColumnEntry {
            column: detail.column.clone(),
            definition: detail.definition.clone(),
            writable: detail.writable,
        };
        if let Cast::Never(reason) = cast(current.cast_kind(), target.cast_kind(), contents) {
            return Err(SchemaError::NeverCasts(reason).into());
        }
        let mut converter = Converter::new(&detail.definition, target, command.clear_invalid);
        for (row_id, value) in values {
            converter.push(row_id, value);
        }
        if let Some(refusal) = converter.refusal(entry.name()) {
            return Err(SchemaError::Misfits(refusal).into());
        }

        let options = validate_option_labels(command.data_type, &converter.labels, &[])?;
        let mut definition = self
            .definitions
            .create_typed_definition(
                database.id,
                &detail.definition.definition.display_name,
                command.data_type,
                command.is_multi_select,
                command.specific_entity_type,
            )
            .await
            .map_err(repository_error)?;
        let new_id = definition.definition.id;
        if !options.is_empty() {
            match self.definitions.add_options(new_id, &options).await {
                Ok(options) => definition.property_options = options,
                Err(error) => {
                    self.delete_unused_definition(new_id).await;
                    return Err(repository_error(error));
                }
            }
        }
        let option_ids: HashMap<_, _> = catalog::option_labels(&definition)
            .into_iter()
            .map(|(id, label)| (label, id))
            .collect();
        let (cleared_cells, trimmed_cells) = (converter.cleared, converter.trimmed);
        let values = converter
            .cells
            .into_iter()
            .map(|(id, value)| {
                (
                    id,
                    match value {
                        ConvertedCell::Value(value) => value,
                        ConvertedCell::Options(labels) => PropertyValue::SelectOption(
                            labels.iter().map(|label| option_ids[label]).collect(),
                        ),
                    },
                )
            })
            .collect();
        let replacement = ColumnReplacement {
            column: detail.column,
            definition_id: new_id,
            config: command
                .relation
                .map(|(database_id, table_id)| ColumnConfig::Link {
                    database_id,
                    table_id,
                }),
            values,
        };
        let table_views = self
            .repository
            .views_for_tables(&[table.id])
            .await
            .map_err(repository_error)?;
        let views = views_without_tests_of(&table_views, replacement.column.id, written_at())?;
        let version = match self.cells.replace_column(table, &replacement, &views).await {
            Ok(Some(version)) => version,
            Ok(None) => {
                self.delete_unused_definition(new_id).await;
                return Err(DatabaseError::VersionConflict);
            }
            // The commit could have succeeded before a transport error. Never
            // delete a potentially bound replacement on an uncertain outcome.
            Err(error) => return Err(repository_error(error)),
        };
        let table_versions = HashMap::from([(table.id, version)]);
        self.publish(attribution, &[(database.id, table.id, version)])
            .await;
        Ok(ColumnTypeChangeOutcome {
            table_versions,
            cleared_cells,
            trimmed_cells,
        })
    }

    /// Why a column's type cannot change at all, whatever it holds: it is
    /// a lookup, a lookup reads through it, or a board groups by it.
    pub(super) async fn retype_blocker(
        &self,
        table_id: TableId,
        detail: &ColumnDetail,
    ) -> Result<Option<SchemaError>, DatabaseError> {
        if matches!(detail.column.config, Some(ColumnConfig::Lookup { .. })) {
            return Ok(Some(SchemaError::RetypeLookup));
        }
        let read_through = self.repository
            .columns_for_tables(&[table_id])
            .await
            .map_err(repository_error)?
            .iter()
            .any(|column| {
                matches!(column.config, Some(ColumnConfig::Lookup { via_column_id, .. }) if via_column_id == detail.column.id)
            });
        if read_through {
            return Ok(Some(SchemaError::LookupBlocksRetype));
        }
        let views = self
            .repository
            .views_for_tables(&[table_id])
            .await
            .map_err(repository_error)?;
        Ok(views_without_tests_of(&views, detail.column.id, written_at()).err())
    }

    pub(super) async fn remove_placement(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        table_id: TableId,
        column_id: ColumnId,
        base_version: TableVersion,
    ) -> Result<ColumnSchemaOutcome, DatabaseError> {
        let (database, tables) = self.database_for_edit(&receipt).await?;
        let table = tables
            .iter()
            .find(|table| table.id == table_id)
            .ok_or(DatabaseError::NotFound)?;
        if table.version != base_version {
            return Err(DatabaseError::VersionConflict);
        }
        let columns = self
            .repository
            .columns_for_tables(&[table_id])
            .await
            .map_err(repository_error)?;
        let column = columns
            .iter()
            .find(|column| column.id == column_id)
            .ok_or(DatabaseError::NotFound)?;
        if columns.iter().any(|column| matches!(column.config, Some(ColumnConfig::Lookup { via_column_id, .. }) if via_column_id == column_id)) {
            return Err(DatabaseError::from(SchemaError::LookupBlocksRemoval));
        }
        let table_views = self
            .repository
            .views_for_tables(&[table_id])
            .await
            .map_err(repository_error)?;
        let views = views_without_column(&table_views, column_id, written_at())?;
        let outcome = self
            .repository
            .delete_column(table, column, &views)
            .await
            .map_err(repository_error)?
            .ok_or(DatabaseError::VersionConflict)?;
        // The repository bumps the table and, for a relation, its target.
        let related = match column.config {
            Some(ColumnConfig::Link {
                database_id,
                table_id,
            }) => Some((database_id, table_id)),
            _ => None,
        };
        let changes: Vec<(DatabaseId, TableId, TableVersion)> = outcome
            .table_versions
            .iter()
            .filter_map(|(changed, version)| {
                let database_id = if *changed == table_id {
                    Some(database.id)
                } else {
                    related
                        .filter(|(_, related_table)| related_table == changed)
                        .map(|(database_id, _)| database_id)
                };
                if database_id.is_none() {
                    tracing::error!(table_id = %changed, "a column removal bumped an unrelated table");
                }
                database_id.map(|database_id| (database_id, *changed, *version))
            })
            .collect();
        self.publish(receipt_attribution(&receipt), &changes).await;
        Ok(outcome)
    }

    pub(super) async fn order_placements(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        table_id: TableId,
        ids: Vec<ColumnId>,
        base_version: TableVersion,
    ) -> Result<ColumnSchemaOutcome, DatabaseError> {
        let (database, tables) = self.database_for_edit(&receipt).await?;
        let table = tables
            .iter()
            .find(|table| table.id == table_id)
            .ok_or(DatabaseError::NotFound)?;
        if table.version != base_version {
            return Err(DatabaseError::VersionConflict);
        }
        let columns = self
            .repository
            .columns_for_tables(&[table_id])
            .await
            .map_err(repository_error)?;
        let expected: HashSet<_> = columns.iter().map(|column| column.id).collect();
        if ids.len() != expected.len() || ids.iter().copied().collect::<HashSet<_>>() != expected {
            return Err(DatabaseError::from(SchemaError::IncompleteColumnOrder));
        }
        let version = self
            .repository
            .reorder_columns(table, &ids)
            .await
            .map_err(repository_error)?
            .ok_or(DatabaseError::VersionConflict)?;
        let table_versions = HashMap::from([(table_id, version)]);
        self.publish(
            receipt_attribution(&receipt),
            &[(database.id, table_id, version)],
        )
        .await;
        Ok(ColumnSchemaOutcome { table_versions })
    }

    pub(super) async fn add_column(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        viewer: Viewer,
        command: CreateColumn,
    ) -> Result<ColumnId, DatabaseError> {
        let (database, tables) = self.database_for_edit(&receipt).await?;
        if !tables.iter().any(|table| table.id == command.table_id) {
            // The receipt covers this database only; a table elsewhere is
            // indistinguishable from a missing one.
            return Err(DatabaseError::NotFound);
        }

        // A link target must be a table the viewer can at least view.
        if let Some(ColumnConfig::Link {
            database_id,
            table_id,
        }) = &command.config
        {
            if self
                .live_database_grant(&viewer, *database_id)
                .await
                .map_err(DatabaseError::Repo)?
                .is_none()
            {
                return Err(DatabaseError::from(SchemaError::LinkDatabaseInaccessible));
            }
            let target_tables = self
                .repository
                .tables_for_databases(&[*database_id])
                .await
                .map_err(repository_error)?;
            if !target_tables.iter().any(|table| table.id == *table_id) {
                return Err(DatabaseError::from(SchemaError::LinkTableMissing));
            }
        }

        if command.infer_type
            && (command.config.is_some()
                || !matches!(
                    &command.binding,
                    ColumnBinding::NewDefinition { data_type: DataType::String, is_multi_select: false, options, .. }
                        if options.is_empty()
                ))
        {
            return Err(DatabaseError::from(SchemaError::InferenceNeedsPlainText));
        }

        // Effective display labels are unique per table, compared as names.
        let existing = self
            .repository
            .columns_for_tables(&[command.table_id])
            .await
            .map_err(repository_error)?;
        let existing_ids: Vec<Uuid> = existing
            .iter()
            .map(|column| column.property_definition_id)
            .collect();
        let definition_names: HashMap<_, _> = self
            .definitions
            .definitions(&existing_ids)
            .await
            .map_err(repository_error)?
            .into_iter()
            .map(|definition| (definition.definition.id, definition.definition.display_name))
            .collect();
        let existing_names: Vec<_> = existing
            .iter()
            .filter_map(|column| {
                column
                    .display_name
                    .as_ref()
                    .or_else(|| definition_names.get(&column.property_definition_id))
            })
            .collect();
        let (binding, option_values) = match command.binding {
            ColumnBinding::NewDefinition {
                name,
                data_type,
                is_multi_select,
                options,
            } => {
                if !options.is_empty() && !takes_options(data_type) {
                    return Err(DatabaseError::from(SchemaError::OptionsOnPlainColumn));
                }
                // Validated before anything is written.
                let values = validate_option_labels(data_type, &options, &[])?;
                (
                    ColumnBinding::NewDefinition {
                        name: validate_name(&name)?,
                        data_type,
                        is_multi_select,
                        options,
                    },
                    values,
                )
            }
            other => (other, Vec::new()),
        };
        if let ColumnBinding::NewDefinition { name, .. } = &binding
            && existing_names
                .iter()
                .any(|existing| same_name(existing, name))
        {
            return Err(DatabaseError::from(SchemaError::ColumnNameTaken {
                name: name.clone(),
            }));
        }
        if let ColumnBinding::ExistingDefinition(id) = &binding
            && existing_ids.contains(id)
        {
            return Err(DatabaseError::from(SchemaError::DefinitionAlreadyBound));
        }

        let definition_id = self
            .definitions
            .resolve_binding(database.id, &viewer, &binding)
            .await
            .map_err(repository_error)?;
        let definition_id = match (definition_id, &binding) {
            (Some(definition_id), _) => definition_id,
            (None, ColumnBinding::ExistingDefinition(id)) => {
                return Err(DatabaseError::from(SchemaError::DefinitionNotFound(*id)));
            }
            (None, ColumnBinding::NewDefinition { .. }) => {
                return Err(DatabaseError::Repo(
                    rootcause::report!("the definition store did not create a new definition")
                        .into_dynamic(),
                ));
            }
        };
        let created = matches!(binding, ColumnBinding::NewDefinition { .. });
        if !option_values.is_empty()
            && let Err(error) = self
                .definitions
                .add_options(definition_id, &option_values)
                .await
        {
            // Nothing binds the new definition yet, so it goes with the failure.
            if created {
                self.delete_unused_definition(definition_id).await;
            }
            return Err(repository_error(error));
        }
        let command = CreateColumn { binding, ..command };
        let (column_id, version) = self
            .repository
            .create_column(command.table_id, definition_id, &command)
            .await
            .map_err(repository_error)?;
        self.publish(
            receipt_attribution(&receipt),
            &[(database.id, command.table_id, version)],
        )
        .await;
        Ok(column_id)
    }

    pub(super) async fn extend_column_options(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        viewer: Viewer,
        command: AddColumnOptions,
    ) -> Result<ColumnDetail, DatabaseError> {
        let (database, tables) = self.database_for_edit(&receipt).await?;
        if !tables.iter().any(|table| table.id == command.table_id) {
            // The receipt covers this database only; a table elsewhere is
            // indistinguishable from a missing one.
            return Err(DatabaseError::NotFound);
        }
        let columns = self
            .repository
            .columns_for_tables(&[command.table_id])
            .await
            .map_err(repository_error)?;
        let column = columns
            .iter()
            .find(|column| column.id == command.column_id)
            .ok_or(DatabaseError::NotFound)?;
        let definition = self
            .definitions
            .definitions(&[column.property_definition_id])
            .await
            .map_err(repository_error)?
            .into_iter()
            .next()
            .ok_or(DatabaseError::NotFound)?;

        let data_type = definition.definition.data_type;
        if !takes_options(data_type) {
            return Err(DatabaseError::from(SchemaError::ColumnTakesNoOptions));
        }
        let entry = catalog::ColumnEntry {
            column: column.clone(),
            definition: definition.clone(),
            writable: true,
        };
        if entry.shared_outside(database.id)
            && !self
                .definitions
                .editable_definitions(&viewer, &[definition.definition.id])
                .await
                .map_err(repository_error)?
                .contains(&definition.definition.id)
        {
            return Err(SchemaError::SharedOptions {
                column: entry.name().to_owned(),
            }
            .into());
        }
        let existing: Vec<String> = definition
            .property_options
            .iter()
            .map(|option| catalog::option_display(&option.value))
            .collect();
        let values = validate_option_labels(data_type, &command.labels, &existing)?;

        // Every label was already there: nothing changed, so nothing is
        // written, versioned, or announced.
        if !values.is_empty() {
            let options: Vec<NewOption> = values
                .into_iter()
                .map(|value| NewOption {
                    definition_id: definition.definition.id,
                    id: macro_uuid::generate_uuid_v7(),
                    value,
                })
                .collect();
            // Options are part of the column's catalog entry, so the table's
            // version moves with them.
            let version = self
                .cells
                .add_options(command.table_id, &options)
                .await
                .map_err(repository_error)?
                .ok_or(DatabaseError::NotFound)?;
            self.publish(
                receipt_attribution(&receipt),
                &[(database.id, command.table_id, version)],
            )
            .await;
        }

        self.column_detail(
            database.id,
            receipt_grant(&receipt, AccessLevel::Edit),
            command.table_id,
            command.column_id,
        )
        .await
    }
}
