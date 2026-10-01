use models_properties::EntityReference;
use models_properties::service::property_value::PropertyValue;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::entity_property_queries;
use super::properties_pg_repo::PropertiesPgRepo;
use super::query_error::PropertyQueryError;
use crate::domain::database_cell_writer::{DatabaseCellWriter, DatabaseWriteTransaction};

impl DatabaseWriteTransaction for PropertiesPgRepo {
    type Transaction = Transaction<'static, Postgres>;
    type Err = PropertyQueryError;
}

impl DatabaseCellWriter for PropertiesPgRepo {
    async fn upsert_entity_property_in(
        &self,
        transaction: &mut Self::Transaction,
        entity: &EntityReference,
        property_definition_id: Uuid,
        value: Option<PropertyValue>,
    ) -> Result<(), Self::Err> {
        entity_property_queries::upsert_entity_property_in_transaction(
            transaction,
            &entity.entity_id,
            entity.entity_type,
            property_definition_id,
            value,
        )
        .await?;
        Ok(())
    }
}
