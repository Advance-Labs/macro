use super::column_types::{ConvertedCell, Converter, is_empty};
use super::views::{views_without_column, views_without_tests_of};
use super::*;
use crate::domain::catalog::{ColumnEntry, PropertyType};
use models_databases::cast::{Cast, Contents, cast};
use models_databases::views::written_at;
use models_properties::service::property_value::PropertyValue;

impl<Repo, Defs, Cells, Events, Access, Broker>
    DatabasesServiceImpl<Repo, Defs, Cells, Events, Access, Broker>
where
    Repo: DatabasesRepo,
    Defs: ColumnDefinitionStore,
    Cells: CellStore,
    Events: TableEventPublisher,
    Access: AccessDirectory,
    Broker: MacroEventBroker,
{
    pub(super) async fn change_placement_type(
        &self,
        receipt: EntityAccessReceipt<EditAccessLevel>,
        viewer: Viewer,
        cmd: ChangeColumnType,
    ) -> Result<ColumnTypeChangeOutcome, DatabaseError> {
        let database_id = receipt_database_id(&receipt)?;
        self.retype_column(database_id, receipt_attribution(&receipt), &viewer, cmd)
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
        cmd: ChangeColumnType,
    ) -> Result<ColumnTypeChangeOutcome, DatabaseError> {
        let (database, tables) = self
            .repo
            .get_database(database_id)
            .await
            .map_err(repo_err)?
            .filter(|(database, _)| database.trashed_at.is_none())
            .ok_or(DatabaseError::NotFound)?;
        let table = tables
            .iter()
            .find(|table| table.id == cmd.table_id)
            .ok_or(DatabaseError::NotFound)?;
        if table.version != cmd.base_version {
            return Err(DatabaseError::VersionConflict);
        }
        if (cmd.specific_entity_type.is_some()
            && (cmd.data_type != DataType::Entity || cmd.relation.is_some()))
            || (cmd.data_type == DataType::Entity
                && cmd.relation.is_none()
                && cmd.specific_entity_type.is_none())
            || (cmd.relation.is_some()
                && (cmd.data_type != DataType::Entity || !cmd.is_multi_select))
            || (cmd.is_multi_select
                && !matches!(
                    cmd.data_type,
                    DataType::SelectString
                        | DataType::SelectNumber
                        | DataType::Tag
                        | DataType::Entity
                        | DataType::Link
                ))
            || (cmd.data_type == DataType::Tag && !cmd.is_multi_select)
        {
            return Err(DatabaseError::from(SchemaError::UnsupportedColumnType));
        }
        if let Some((database_id, table_id)) = cmd.relation
            && (self
                .live_database_grant(viewer, database_id)
                .await
                .map_err(DatabaseError::Repo)?
                .is_none()
                || !self
                    .repo
                    .tables_for_databases(&[database_id])
                    .await
                    .map_err(repo_err)?
                    .iter()
                    .any(|table| table.id == table_id))
        {
            return Err(DatabaseError::from(SchemaError::RelatedTableInaccessible));
        }
        let detail = self
            .column_detail(
                viewer,
                database.id,
                AccessLevel::Edit,
                table.id,
                cmd.column_id,
            )
            .await?;
        if let Some(blocker) = self.retype_blocker(table.id, &detail).await? {
            return Err(blocker.into());
        }
        let current = PropertyType::of(&detail.column, &detail.definition);
        let target = PropertyType {
            data_type: cmd.data_type,
            is_multi_select: cmd.is_multi_select,
            specific_entity_type: cmd.specific_entity_type,
            relation: cmd.relation.is_some(),
        };
        let same_relation = match (&detail.column.config, cmd.relation) {
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
        let mut converter = Converter::new(&detail.definition, target, cmd.clear_invalid);
        for (row_id, value) in values {
            converter.push(row_id, value);
        }
        if let Some(refusal) = converter.refusal(entry.name()) {
            return Err(SchemaError::Misfits(refusal).into());
        }

        let options = validate_option_labels(cmd.data_type, &converter.labels, &[])?;
        let mut definition = self
            .definitions
            .create_typed_definition(
                database.id,
                &detail.definition.definition.display_name,
                cmd.data_type,
                cmd.is_multi_select,
                cmd.specific_entity_type,
            )
            .await
            .map_err(repo_err)?;
        let new_id = definition.definition.id;
        if !options.is_empty() {
            match self.definitions.add_options(new_id, &options).await {
                Ok(options) => definition.property_options = options,
                Err(error) => {
                    self.delete_unused_definition(new_id).await;
                    return Err(repo_err(error));
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
            config: cmd
                .relation
                .map(|(database_id, table_id)| ColumnConfig::Link {
                    database_id,
                    table_id,
                }),
            values,
        };
        let table_views = self
            .repo
            .views_for_tables(&[table.id])
            .await
            .map_err(repo_err)?;
        let views = views_without_tests_of(&table_views, replacement.column.id, written_at())?;
        let version = match self.cells.replace_column(table, &replacement, &views).await {
            Ok(Some(version)) => version,
            Ok(None) => {
                self.delete_unused_definition(new_id).await;
                return Err(DatabaseError::VersionConflict);
            }
            // The commit could have succeeded before a transport error. Never
            // delete a potentially bound replacement on an uncertain outcome.
            Err(error) => return Err(repo_err(error)),
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
        let read_through = self
            .repo
            .columns_for_tables(&[table_id])
            .await
            .map_err(repo_err)?
            .iter()
            .any(|column| {
                matches!(column.config, Some(ColumnConfig::Lookup { via_column_id, .. }) if via_column_id == detail.column.id)
            });
        if read_through {
            return Ok(Some(SchemaError::LookupBlocksRetype));
        }
        let views = self
            .repo
            .views_for_tables(&[table_id])
            .await
            .map_err(repo_err)?;
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
            .repo
            .columns_for_tables(&[table_id])
            .await
            .map_err(repo_err)?;
        let column = columns
            .iter()
            .find(|column| column.id == column_id)
            .ok_or(DatabaseError::NotFound)?;
        if columns.iter().any(|column| matches!(column.config, Some(ColumnConfig::Lookup { via_column_id, .. }) if via_column_id == column_id)) {
            return Err(DatabaseError::from(SchemaError::LookupBlocksRemoval));
        }
        let table_views = self
            .repo
            .views_for_tables(&[table_id])
            .await
            .map_err(repo_err)?;
        let views = views_without_column(&table_views, column_id, written_at())?;
        let outcome = self
            .repo
            .delete_column(table, column, &views)
            .await
            .map_err(repo_err)?
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
            .repo
            .columns_for_tables(&[table_id])
            .await
            .map_err(repo_err)?;
        let expected: HashSet<_> = columns.iter().map(|column| column.id).collect();
        if ids.len() != expected.len() || ids.iter().copied().collect::<HashSet<_>>() != expected {
            return Err(DatabaseError::from(SchemaError::IncompleteColumnOrder));
        }
        let version = self
            .repo
            .reorder_columns(table, &ids)
            .await
            .map_err(repo_err)?
            .ok_or(DatabaseError::VersionConflict)?;
        let table_versions = HashMap::from([(table_id, version)]);
        self.publish(
            receipt_attribution(&receipt),
            &[(database.id, table_id, version)],
        )
        .await;
        Ok(ColumnSchemaOutcome { table_versions })
    }
}
