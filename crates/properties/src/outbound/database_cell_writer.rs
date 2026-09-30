use models_properties::EntityReference;
use models_properties::option_color;
use models_properties::service::property_option::PropertyOptionValue;
use models_properties::service::property_value::PropertyValue;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::entity_property_queries;
use super::properties_pg_repo::PropertiesPgRepo;
use super::property_option_queries;
use crate::domain::database_cell_writer::DatabaseCellWriter;

/// A cell or option write failed; the caller's transaction is to be dropped.
#[derive(Debug, thiserror::Error)]
#[error("database cell write failed: {0}")]
pub struct DatabaseCellWriteError(#[from] anyhow::Error);

impl DatabaseCellWriter for PropertiesPgRepo {
    type Transaction = Transaction<'static, Postgres>;
    type Err = DatabaseCellWriteError;

    async fn add_options_in(
        &self,
        transaction: &mut Self::Transaction,
        property_definition_id: Uuid,
        options: &[(Uuid, PropertyOptionValue)],
    ) -> Result<(), Self::Err> {
        if options.is_empty() {
            return Ok(());
        }
        let existing = property_option_queries::get_property_options(
            &mut **transaction,
            property_definition_id,
        )
        .await?;
        let first_order = existing
            .iter()
            .map(|option| option.display_order)
            .max()
            .map_or(0, |highest| highest + 1);
        for (index, (id, value)) in options.iter().enumerate() {
            property_option_queries::insert_property_option(
                &mut **transaction,
                *id,
                property_definition_id,
                first_order + index as i32,
                value.clone(),
                Some(option_color(existing.len() + index).to_string()),
            )
            .await?;
        }
        Ok(())
    }

    async fn upsert_entity_property_in(
        &self,
        transaction: &mut Self::Transaction,
        entity: &EntityReference,
        property_definition_id: Uuid,
        value: Option<PropertyValue>,
    ) -> Result<(), Self::Err> {
        entity_property_queries::upsert_entity_property(
            &mut **transaction,
            &entity.entity_id,
            entity.entity_type,
            property_definition_id,
            value,
        )
        .await?;
        Ok(())
    }

    async fn delete_entity_properties_in(
        &self,
        transaction: &mut Self::Transaction,
        entity: &EntityReference,
    ) -> Result<(), Self::Err> {
        entity_property_queries::delete_entity_properties(&mut **transaction, entity).await?;
        Ok(())
    }
}
