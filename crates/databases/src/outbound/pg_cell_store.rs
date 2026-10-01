//! Cells as entity properties, through the properties crate's own Postgres
//! adapter: the `entity_properties` table stays that crate's to write. A
//! batch of writes runs on one transaction that the row identities, the
//! cells and the select options all share.

use std::collections::HashMap;

use models_properties::service::property_value::PropertyValue;
use models_properties::{EntityReference, EntityType};
use properties::domain::database_cell_writer::DatabaseCellWriter;
use properties::domain::model::UpdatePropertyOptionOutcome;
use properties::domain::ports::PropertiesRepo;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::models::{PropertyDefinitionId, RowId, TableId, Write, Writes, WritesOutcome};
use crate::domain::ports::CellStore;
use crate::outbound::pg_databases_repo::rows;

/// [`CellStore`] over the properties repository, with the pool its batches
/// open their transaction on.
#[derive(Debug, Clone)]
pub struct PgCellStore<Properties> {
    pool: PgPool,
    properties: Properties,
}

impl<Properties> PgCellStore<Properties> {
    /// Wrap the properties repository.
    pub fn new(pool: PgPool, properties: Properties) -> Self {
        Self { pool, properties }
    }
}

/// The properties-side name of a row.
fn row_entity(row: RowId) -> EntityReference {
    EntityReference {
        entity_id: row.to_string(),
        entity_type: EntityType::DatabaseRow,
        specific_message_id: None,
    }
}

/// The store's error, keeping the failing side's cause.
#[derive(Debug, thiserror::Error)]
pub enum PgCellStoreError {
    /// The properties repository failed.
    #[error("properties: {0}")]
    Properties(#[from] anyhow::Error),
    /// A statement of a batch failed; nothing of it committed.
    #[error("row batch: {0}")]
    Sqlx(#[from] sqlx::Error),
    /// The properties writer failed inside a batch; nothing of it committed.
    #[error("row batch cells: {0}")]
    Cells(#[source] Box<dyn std::error::Error + Send + Sync>),
}

fn cells_error(error: impl std::error::Error + Send + Sync + 'static) -> PgCellStoreError {
    PgCellStoreError::Cells(Box::new(error))
}

impl<Properties> CellStore for PgCellStore<Properties>
where
    Properties: PropertiesRepo<Err = anyhow::Error>
        + DatabaseCellWriter<Transaction = Transaction<'static, Postgres>>
        + Send
        + Sync
        + 'static,
{
    type Err = PgCellStoreError;

    #[tracing::instrument(err, skip(self, rows), fields(rows = rows.len()))]
    async fn cells(
        &self,
        rows: &[RowId],
    ) -> Result<HashMap<RowId, HashMap<PropertyDefinitionId, PropertyValue>>, Self::Err> {
        if rows.is_empty() {
            return Ok(HashMap::new());
        }
        let fetched = self
            .properties
            .get_entity_properties_batch(rows.iter().map(|row| row_entity(*row)).collect())
            .await?;
        let mut cells: HashMap<RowId, HashMap<PropertyDefinitionId, PropertyValue>> =
            HashMap::new();
        for (key, properties) in fetched {
            let Ok(row) = Uuid::parse_str(&key.entity_id) else {
                continue;
            };
            let row_cells = cells.entry(row).or_default();
            for property in properties {
                if let Some(value) = property.value {
                    row_cells.insert(property.property.property_definition_id, value);
                }
            }
        }
        Ok(cells)
    }

    #[tracing::instrument(err, skip(self, cells), fields(cells = cells.len()))]
    async fn write(
        &self,
        row: RowId,
        cells: &[(PropertyDefinitionId, Option<PropertyValue>)],
    ) -> Result<(), Self::Err> {
        let entity_id = row.to_string();
        for (definition, value) in cells {
            self.properties
                .upsert_entity_property(
                    &entity_id,
                    EntityType::DatabaseRow,
                    *definition,
                    value.clone(),
                )
                .await?;
        }
        Ok(())
    }

    #[tracing::instrument(err, skip(self))]
    async fn clear(&self, row: RowId) -> Result<(), Self::Err> {
        self.properties
            .delete_entity_properties(&row_entity(row))
            .await?;
        Ok(())
    }

    #[tracing::instrument(err, skip(self, writes), fields(writes = writes.writes.len()))]
    async fn apply_writes(&self, writes: &Writes) -> Result<WritesOutcome, Self::Err> {
        // Returning before the commit drops the transaction, which rolls
        // everything back.
        let mut transaction = self.pool.begin().await?;

        let mut tables: Vec<TableId> = writes
            .writes
            .iter()
            .flat_map(|write| write.versioned_tables().iter().copied())
            .collect();
        tables.sort();
        tables.dedup();
        let live = rows::lock_live_tables(&mut *transaction, &tables).await?;
        if let Some(gone) = tables.iter().find(|table| !live.contains(table)) {
            return Ok(WritesOutcome::TableNotFound(*gone));
        }

        let mut options: Vec<(PropertyDefinitionId, Vec<_>)> = Vec::new();
        for option in &writes.options {
            let value = (option.id, option.value.clone());
            match options
                .iter_mut()
                .find(|(definition, _)| *definition == option.definition_id)
            {
                Some((_, values)) => values.push(value),
                None => options.push((option.definition_id, vec![value])),
            }
        }
        for (definition, values) in &options {
            self.properties
                .add_options_in(&mut transaction, *definition, values)
                .await
                .map_err(cells_error)?;
        }

        let mut inserted = Vec::with_capacity(writes.writes.len());
        for (index, write) in writes.writes.iter().enumerate() {
            match write {
                Write::InsertRows { table_id, rows } => {
                    let Some(minted) = rows::append_rows(
                        &mut transaction,
                        *table_id,
                        &writes.created_by,
                        rows.len(),
                    )
                    .await?
                    else {
                        return Ok(WritesOutcome::TableNotFound(*table_id));
                    };
                    let mut valued = Vec::new();
                    for (row, cells) in minted.iter().zip(rows) {
                        for (definition, value) in cells {
                            self.properties
                                .upsert_entity_property_in(
                                    &mut transaction,
                                    &row_entity(row.id),
                                    *definition,
                                    Some(value.clone()),
                                )
                                .await
                                .map_err(cells_error)?;
                            if !valued.contains(definition) {
                                valued.push(*definition);
                            }
                        }
                    }
                    rows::settle_inference(&mut *transaction, *table_id, &valued).await?;
                    inserted.push(minted.into_iter().map(|row| row.id).collect());
                }
                Write::UpdateRows { table_id, rows } => {
                    let named: Vec<RowId> = rows.iter().map(|(row, _)| *row).collect();
                    let owned = rows::lock_rows(&mut *transaction, *table_id, &named).await?;
                    if let Some(row) = named.iter().find(|row| !owned.contains(row)) {
                        return Ok(WritesOutcome::MissingRow {
                            write: index,
                            row: *row,
                        });
                    }
                    let mut valued = Vec::new();
                    for (row, cells) in rows {
                        for (definition, value) in cells {
                            self.properties
                                .upsert_entity_property_in(
                                    &mut transaction,
                                    &row_entity(*row),
                                    *definition,
                                    value.clone(),
                                )
                                .await
                                .map_err(cells_error)?;
                            if value.is_some() && !valued.contains(definition) {
                                valued.push(*definition);
                            }
                        }
                    }
                    rows::settle_inference(&mut *transaction, *table_id, &valued).await?;
                    inserted.push(Vec::new());
                }
                Write::DeleteRows { table_id, rows } => {
                    for row in rows {
                        if !rows::delete_row(&mut *transaction, *table_id, *row).await? {
                            return Ok(WritesOutcome::MissingRow {
                                write: index,
                                row: *row,
                            });
                        }
                        self.properties
                            .delete_entity_properties_in(&mut transaction, &row_entity(*row))
                            .await
                            .map_err(cells_error)?;
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
                    match self
                        .properties
                        .update_option_in(
                            &mut transaction,
                            *definition_id,
                            *option_id,
                            value.clone(),
                            color.clone(),
                        )
                        .await
                        .map_err(cells_error)?
                    {
                        UpdatePropertyOptionOutcome::Updated(_) => {}
                        UpdatePropertyOptionOutcome::NotFound => {
                            return Ok(WritesOutcome::MissingOption { write: index });
                        }
                        UpdatePropertyOptionOutcome::DuplicateValue => {
                            return Ok(WritesOutcome::OptionLabelTaken { write: index });
                        }
                    }
                    inserted.push(Vec::new());
                }
                Write::DeleteOption {
                    definition_id,
                    option_id,
                    ..
                } => {
                    if !self
                        .properties
                        .delete_option_in(&mut transaction, *definition_id, *option_id)
                        .await
                        .map_err(cells_error)?
                    {
                        return Ok(WritesOutcome::MissingOption { write: index });
                    }
                    inserted.push(Vec::new());
                }
            }
        }

        let mut related: Vec<(TableId, Vec<RowId>)> = Vec::new();
        for (table, row) in &writes.related_rows {
            match related.iter_mut().find(|(target, _)| target == table) {
                Some((_, rows)) => rows.push(*row),
                None => related.push((*table, vec![*row])),
            }
        }
        for (table, named) in &related {
            let held = rows::hold_rows(&mut *transaction, *table, named).await?;
            if let Some(row) = named.iter().find(|row| !held.contains(row)) {
                return Ok(WritesOutcome::MissingRelatedRow(*row));
            }
        }

        let mut table_versions = HashMap::new();
        for table in tables {
            let changed = writes
                .writes
                .iter()
                .any(|write| write.changes() && write.versioned_tables().contains(&table));
            if changed {
                let version = rows::bump_table_version(&mut *transaction, table).await?;
                table_versions.insert(table, version);
            }
        }
        transaction.commit().await?;
        Ok(WritesOutcome::Applied {
            inserted,
            table_versions,
        })
    }
}
