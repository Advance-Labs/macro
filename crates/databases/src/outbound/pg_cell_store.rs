//! Cells as entity properties through the properties crate's adapter; a batch
//! shares one transaction across row identities, cells and options.

mod transfer;

use std::collections::HashMap;

use models_properties::service::property_value::PropertyValue;
use models_properties::{EntityReference, EntityType};
use properties::domain::database_cell_writer::{ColorChange, DatabaseCellWriter};
use properties::domain::model::UpdatePropertyOptionOutcome;
use properties::domain::ports::PropertiesRepo;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::models::{
    ColumnReplacement, DatabaseView, NewOption, PropertyDefinitionId, RowId, Table, TableId,
    TableVersion, Write, Writes, WritesOutcome,
};
use crate::domain::ports::CellStore;
use crate::outbound::pg_databases_repo::columns::{lock_column_tables, rebind_placement};
use crate::outbound::pg_databases_repo::{PgDatabasesRepoError, rows, views};

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
    /// A repository statement of a batch failed; nothing of it committed.
    #[error("row batch statement: {0}")]
    Repository(#[from] PgDatabasesRepoError),
    /// The properties side keyed a row's cells by something that is not a
    /// row id.
    #[error("row cells keyed by `{0}`, which is not a row id")]
    CorruptRowId(String),
    /// An imported table carries its request key without the fingerprint
    /// written with it.
    #[error("imported table {0} has no import fingerprint")]
    MissingImportFingerprint(TableId),
}

/// Group new options by the definition they join, keeping their order.
fn options_by_definition(
    options: &[NewOption],
) -> Vec<(
    PropertyDefinitionId,
    Vec<(
        Uuid,
        models_properties::service::property_option::PropertyOptionValue,
    )>,
)> {
    let mut grouped: Vec<(PropertyDefinitionId, Vec<_>)> = Vec::new();
    for option in options {
        let value = (option.id.into_uuid(), option.value.clone());
        match grouped
            .iter_mut()
            .find(|(definition, _)| *definition == option.definition_id)
        {
            Some((_, values)) => values.push(value),
            None => grouped.push((option.definition_id, vec![value])),
        }
    }
    grouped
}

/// The unique index on a table's view names.
const VIEW_NAME_CONSTRAINT: &str = "database_views_table_name_key";

/// Whether a view statement failed on the unique view name of its table.
fn name_taken(error: &PgDatabasesRepoError) -> bool {
    matches!(
        error,
        PgDatabasesRepoError::Sqlx(sqlx::Error::Database(database))
            if database.constraint() == Some(VIEW_NAME_CONSTRAINT)
    )
}

/// The row a properties-side entity id names.
fn row_of(entity_id: &str) -> Result<RowId, PgCellStoreError> {
    entity_id
        .parse()
        .map_err(|_| PgCellStoreError::CorruptRowId(entity_id.to_string()))
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
    type Error = PgCellStoreError;

    #[tracing::instrument(err, skip(self, rows), fields(rows = rows.len()))]
    async fn cells(
        &self,
        rows: &[RowId],
    ) -> Result<HashMap<RowId, HashMap<PropertyDefinitionId, PropertyValue>>, Self::Error> {
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
            let row_cells = cells.entry(row_of(&key.entity_id)?).or_default();
            for property in properties {
                if let Some(value) = property.value {
                    row_cells.insert(property.property.property_definition_id, value);
                }
            }
        }
        Ok(cells)
    }

    #[tracing::instrument(err, skip(self, rows), fields(rows = rows.len()))]
    async fn column_cells(
        &self,
        rows: &[RowId],
        definition: PropertyDefinitionId,
    ) -> Result<HashMap<RowId, PropertyValue>, Self::Error> {
        if rows.is_empty() {
            return Ok(HashMap::new());
        }
        let fetched = self
            .properties
            .get_entity_properties_batch_filtered(
                rows.iter().map(|row| row_entity(*row)).collect(),
                vec![definition],
                None,
            )
            .await?;
        let mut cells = HashMap::new();
        for (key, properties) in fetched {
            let row = row_of(&key.entity_id)?;
            let value = properties
                .into_iter()
                .find(|property| property.property.property_definition_id == definition)
                .and_then(|property| property.value);
            if let Some(value) = value {
                cells.insert(row, value);
            }
        }
        Ok(cells)
    }

    #[tracing::instrument(err, skip(self, replacement, views), fields(cells = replacement.values.len()))]
    async fn replace_column(
        &self,
        table: &Table,
        replacement: &ColumnReplacement,
        views: &[DatabaseView],
    ) -> Result<Option<TableVersion>, Self::Error> {
        let Some(mut transaction) = lock_column_tables(&self.pool, table, &[table.id]).await?
        else {
            return Ok(None);
        };
        if !rebind_placement(&mut transaction, table, replacement, views).await? {
            transaction.rollback().await?;
            return Ok(None);
        }
        // The schema dropped the old definition's cells with the rebind; the
        // converted ones land in the same transaction.
        for (row, value) in &replacement.values {
            self.properties
                .upsert_entity_property_in(
                    &mut transaction,
                    &row_entity(*row),
                    replacement.definition_id,
                    Some(value.clone()),
                )
                .await
                .map_err(cells_error)?;
        }
        let version = rows::bump_table_version(&mut *transaction, table.id).await?;
        transaction.commit().await?;
        Ok(Some(version))
    }

    #[tracing::instrument(err, skip(self, options), fields(options = options.len()))]
    async fn add_options(
        &self,
        table_id: TableId,
        options: &[NewOption],
    ) -> Result<Option<TableVersion>, Self::Error> {
        let mut transaction = self.pool.begin().await?;
        if rows::lock_live_tables(&mut *transaction, &[table_id])
            .await?
            .is_empty()
        {
            return Ok(None);
        }
        for (definition, values) in options_by_definition(options) {
            self.properties
                .add_options_in(&mut transaction, definition, &values)
                .await
                .map_err(cells_error)?;
        }
        let version = rows::bump_table_version(&mut *transaction, table_id).await?;
        transaction.commit().await?;
        Ok(Some(version))
    }

    #[tracing::instrument(err, skip(self, writes), fields(writes = writes.writes.len()))]
    async fn apply_writes(&self, writes: &Writes) -> Result<WritesOutcome, Self::Error> {
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

        let options = options_by_definition(&writes.options);
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
                        writes.created_by.as_ref(),
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
                        // The row's cells go with it, by the schema's trigger.
                        if !rows::delete_row(&mut *transaction, *table_id, *row).await? {
                            return Ok(WritesOutcome::MissingRow {
                                write: index,
                                row: *row,
                            });
                        }
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
                            option_id.into_uuid(),
                            value.clone(),
                            match color {
                                None => ColorChange::Keep,
                                Some(None) => ColorChange::Clear,
                                Some(Some(color)) => ColorChange::Set(color.clone()),
                            },
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
                    tables,
                    definition_id,
                    option_id,
                    views: rewritten,
                    ..
                } => {
                    if !self
                        .properties
                        .delete_option_in(&mut transaction, *definition_id, option_id.into_uuid())
                        .await
                        .map_err(cells_error)?
                    {
                        return Ok(WritesOutcome::MissingOption { write: index });
                    }
                    for view in rewritten {
                        if !views::update_view(&mut *transaction, view).await? {
                            return Ok(WritesOutcome::MissingView { write: index });
                        }
                    }
                    views::clear_lane(&mut *transaction, tables, *option_id).await?;
                    inserted.push(Vec::new());
                }
                Write::CreateView { view } => {
                    match views::insert_view(&mut *transaction, view).await {
                        Ok(()) => {}
                        Err(error) if name_taken(&error) => {
                            return Ok(WritesOutcome::ViewNameTaken { write: index });
                        }
                        Err(error) => return Err(error.into()),
                    }
                    inserted.push(Vec::new());
                }
                Write::UpdateView { view, regrouped } => {
                    match views::update_view(&mut *transaction, view).await {
                        Ok(true) => {}
                        Ok(false) => return Ok(WritesOutcome::MissingView { write: index }),
                        Err(error) if name_taken(&error) => {
                            return Ok(WritesOutcome::ViewNameTaken { write: index });
                        }
                        Err(error) => return Err(error.into()),
                    }
                    if *regrouped {
                        views::clear_positions(&mut *transaction, view.id).await?;
                    }
                    inserted.push(Vec::new());
                }
                Write::DeleteView { table_id, view_id } => {
                    if !views::delete_view(&mut *transaction, *table_id, *view_id).await? {
                        return Ok(WritesOutcome::MissingView { write: index });
                    }
                    inserted.push(Vec::new());
                }
                Write::OrderViews {
                    table_id,
                    positions,
                } => {
                    if !views::order_views(&mut *transaction, *table_id, positions).await? {
                        return Ok(WritesOutcome::MissingView { write: index });
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
                    if rows::lock_rows(&mut *transaction, *table_id, &[*row])
                        .await?
                        .is_empty()
                    {
                        return Ok(WritesOutcome::MissingRow {
                            write: index,
                            row: *row,
                        });
                    }
                    self.properties
                        .upsert_entity_property_in(
                            &mut transaction,
                            &row_entity(*row),
                            *definition,
                            value.clone(),
                        )
                        .await
                        .map_err(cells_error)?;
                    views::place_cards(&mut *transaction, *view_id, positions).await?;
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
