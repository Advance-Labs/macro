//! Cells as entity properties, through the properties crate's own Postgres
//! adapter: the `entity_properties` table stays that crate's to write.

use std::collections::HashMap;

use models_properties::service::property_value::PropertyValue;
use models_properties::{EntityReference, EntityType};
use properties::domain::ports::PropertiesRepo;
use uuid::Uuid;

use crate::domain::models::{PropertyDefinitionId, RowId};
use crate::domain::ports::CellStore;

/// [`CellStore`] over the properties repository.
#[derive(Debug, Clone)]
pub struct PgCellStore<Properties> {
    properties: Properties,
}

impl<Properties> PgCellStore<Properties> {
    /// Wrap the properties repository.
    pub fn new(properties: Properties) -> Self {
        Self { properties }
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

/// The store's error: whatever the properties repository reports.
#[derive(Debug, thiserror::Error)]
#[error("properties: {0}")]
pub struct PgCellStoreError(#[from] anyhow::Error);

impl<Properties> CellStore for PgCellStore<Properties>
where
    Properties: PropertiesRepo<Err = anyhow::Error> + Send + Sync + 'static,
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
}
