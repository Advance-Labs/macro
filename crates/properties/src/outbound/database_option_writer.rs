use models_properties::service::property_option::PropertyOptionValue;
use uuid::Uuid;

use super::database_definition_writer::display_order;
use super::properties_pg_repo::PropertiesPgRepo;
use super::property_option_queries;
use super::query_error::PropertyQueryError;
use crate::TagColor;
use crate::domain::database_option_writer::{ColorChange, DatabaseOptionWriter};
use crate::domain::model::UpdatePropertyOptionOutcome;

impl DatabaseOptionWriter for PropertiesPgRepo {
    async fn add_options_in(
        &self,
        transaction: &mut Self::Transaction,
        property_definition_id: Uuid,
        options: &[(Uuid, PropertyOptionValue)],
    ) -> Result<(), Self::Err> {
        if options.is_empty() {
            return Ok(());
        }
        // Two appends read the same last place without it.
        sqlx::query_scalar!(
            "SELECT id FROM property_definitions WHERE id = $1 FOR UPDATE",
            property_definition_id
        )
        .fetch_optional(&mut **transaction)
        .await?;
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
        for (offset, (id, value)) in options.iter().enumerate() {
            let display_order = display_order(offset)?
                .checked_add(first_order)
                .ok_or(PropertyQueryError::DisplayOrderOverflow(offset))?;
            property_option_queries::insert_property_option(
                &mut **transaction,
                *id,
                property_definition_id,
                display_order,
                value.clone(),
                Some(
                    TagColor::for_position(existing.len() + offset)
                        .hex()
                        .to_string(),
                ),
            )
            .await?;
        }
        Ok(())
    }

    async fn existing_option_ids_in(
        &self,
        transaction: &mut Self::Transaction,
        ids: &[Uuid],
    ) -> Result<Vec<Uuid>, Self::Err> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(
            sqlx::query_scalar!("SELECT id FROM property_options WHERE id = ANY($1)", ids)
                .fetch_all(&mut **transaction)
                .await?,
        )
    }

    async fn update_option_in(
        &self,
        transaction: &mut Self::Transaction,
        property_definition_id: Uuid,
        option_id: Uuid,
        value: Option<PropertyOptionValue>,
        color: ColorChange,
    ) -> Result<UpdatePropertyOptionOutcome, Self::Err> {
        property_option_queries::patch_property_option(
            &mut **transaction,
            property_definition_id,
            option_id,
            value,
            color,
        )
        .await
    }

    async fn delete_option_in(
        &self,
        transaction: &mut Self::Transaction,
        property_definition_id: Uuid,
        option_id: Uuid,
    ) -> Result<bool, Self::Err> {
        let deleted = property_option_queries::delete_options_in_tx(
            transaction,
            property_definition_id,
            &[option_id],
        )
        .await?;
        property_option_queries::clear_emptied_database_row_values(
            &mut **transaction,
            property_definition_id,
        )
        .await?;
        Ok(deleted == 1)
    }
}
