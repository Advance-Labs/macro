//! The [`ColumnDefinitionStore`] port over the properties domain.
//!
//! A database column IS a `property_definitions` row. Definitions created for a
//! column are owned by the database (`database_id` set, `user_id`/`team_id`
//! NULL, `is_system` false), which keeps them out of the shared user/team
//! property namespace — see
//! `crates/macro_db_client/migrations/20260929191815_add_database_property_owner.up.sql`.
//!
//! Mechanics only: policy (who may bind what) lives in the domain service.

#[cfg(test)]
mod test;

use models_properties::service::property_definition_with_options::PropertyDefinitionWithOptions;
use models_properties::service::property_option::{PropertyOption, PropertyOptionValue};
use models_properties::{DataType, EntityType, option_color};
use properties::domain::ports::PropertiesRepo;

use crate::domain::models::{ColumnBinding, DatabaseId, PropertyDefinitionId, Viewer};
use crate::domain::ports::ColumnDefinitionStore;

/// Errors from the column-definition store.
#[derive(Debug, thiserror::Error)]
pub enum PgDefinitionStoreError {
    /// Failure from the owning properties domain.
    #[error("properties error: {0}")]
    Properties(#[source] anyhow::Error),
    /// A binding referenced a property definition that does not exist, or
    /// one the viewer may not bind.
    #[error("property definition {0} not found")]
    NotFound(PropertyDefinitionId),
}

/// [`ColumnDefinitionStore`] over the properties domain's repository, which
/// owns `property_definitions` and `property_options`.
#[derive(Debug, Clone)]
pub struct PgDefinitionStore<P> {
    properties: P,
}

impl<P: PropertiesRepo<Err = anyhow::Error>> PgDefinitionStore<P> {
    /// Create a store over the owning properties domain port.
    pub fn new(properties: P) -> Self {
        Self { properties }
    }

    /// Insert a definition owned by `database_id`, returning its id.
    async fn create_database_definition(
        &self,
        database_id: DatabaseId,
        name: &str,
        data_type: DataType,
        is_multi_select: bool,
    ) -> Result<PropertyDefinitionId, PgDefinitionStoreError> {
        self.properties
            .create_database_property_definition(
                database_id,
                name,
                data_type,
                is_multi_select,
                None,
            )
            .await
            .map(|definition| definition.id)
            .map_err(PgDefinitionStoreError::Properties)
    }
}

impl<P: PropertiesRepo<Err = anyhow::Error>> ColumnDefinitionStore for PgDefinitionStore<P> {
    type Err = PgDefinitionStoreError;

    #[tracing::instrument(skip(self, viewer, binding), err)]
    async fn resolve_binding(
        &self,
        database_id: DatabaseId,
        viewer: &Viewer,
        binding: &ColumnBinding,
    ) -> Result<PropertyDefinitionId, Self::Err> {
        match binding {
            // Options are attached separately, through
            // [`ColumnDefinitionStore::add_options`], once the definition exists.
            ColumnBinding::NewDefinition {
                name,
                data_type,
                is_multi_select,
                options: _,
            } => {
                self.create_database_definition(database_id, name, *data_type, *is_multi_select)
                    .await
            }
            ColumnBinding::ExistingDefinition(id) => self
                .properties
                .get_bindable_property_definition(*id, viewer.user_id.as_ref(), database_id)
                .await
                .map_err(PgDefinitionStoreError::Properties)?
                .map(|definition| definition.id)
                .ok_or(PgDefinitionStoreError::NotFound(*id)),
        }
    }

    async fn create_typed_definition(
        &self,
        database_id: DatabaseId,
        name: &str,
        data_type: DataType,
        is_multi_select: bool,
        specific_entity_type: Option<EntityType>,
    ) -> Result<PropertyDefinitionWithOptions, Self::Err> {
        let definition = self
            .properties
            .create_database_property_definition(
                database_id,
                name,
                data_type,
                is_multi_select,
                specific_entity_type,
            )
            .await
            .map_err(PgDefinitionStoreError::Properties)?;
        Ok(PropertyDefinitionWithOptions {
            definition,
            property_options: Vec::new(),
        })
    }

    async fn delete_unused_definition(&self, id: PropertyDefinitionId) -> Result<(), Self::Err> {
        self.properties
            .delete_property_definition(id)
            .await
            .map_err(PgDefinitionStoreError::Properties)
    }

    #[tracing::instrument(skip(self), err)]
    async fn add_options(
        &self,
        definition_id: PropertyDefinitionId,
        values: &[PropertyOptionValue],
    ) -> Result<Vec<PropertyOption>, Self::Err> {
        if values.is_empty() {
            return Ok(Vec::new());
        }
        // New options go after the ones already there, so the order the user
        // sees (and the labels the catalog derives from it) is stable.
        let existing = self
            .properties
            .get_property_options(definition_id)
            .await
            .map_err(PgDefinitionStoreError::Properties)?;
        let mut display_order = existing
            .iter()
            .map(|option| option.display_order)
            .max()
            .map_or(0, |highest| highest + 1);

        let mut created = Vec::with_capacity(values.len());
        for (index, value) in values.iter().enumerate() {
            let color = option_color(existing.len() + index).to_string();
            created.push(
                self.properties
                    .create_property_option(
                        definition_id,
                        display_order,
                        value.clone(),
                        Some(color),
                    )
                    .await
                    .map_err(PgDefinitionStoreError::Properties)?,
            );
            display_order += 1;
        }
        Ok(created)
    }

    #[tracing::instrument(skip(self), err)]
    async fn definitions(
        &self,
        ids: &[PropertyDefinitionId],
    ) -> Result<Vec<PropertyDefinitionWithOptions>, Self::Err> {
        self.properties
            .get_property_definitions_with_options(ids)
            .await
            .map_err(PgDefinitionStoreError::Properties)
    }
}
