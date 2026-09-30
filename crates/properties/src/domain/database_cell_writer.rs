//! Transactional composition for database rows' cells and their select
//! options, so a batch of row writes commits or rolls back as one.

use models_properties::EntityReference;
use models_properties::service::property_option::PropertyOptionValue;
use models_properties::service::property_value::PropertyValue;
use uuid::Uuid;

/// Lets a composition root write entity properties and select options within
/// an owning use case's transaction. The transaction stays opaque; adapters
/// choose its implementation.
pub trait DatabaseCellWriter: Send + Sync + 'static {
    /// Adapter-owned transaction handle.
    type Transaction: Send;
    /// Persistence or decoding error.
    type Err: std::error::Error + Send + Sync + 'static;

    /// Append options, under the ids given, after a definition's existing
    /// ones. Each takes the palette colour of its position
    /// ([`models_properties::option_color`]).
    fn add_options_in(
        &self,
        transaction: &mut Self::Transaction,
        property_definition_id: Uuid,
        options: &[(Uuid, PropertyOptionValue)],
    ) -> impl Future<Output = Result<(), Self::Err>> + Send;

    /// Set one entity property, or with `None` clear its value.
    fn upsert_entity_property_in(
        &self,
        transaction: &mut Self::Transaction,
        entity: &EntityReference,
        property_definition_id: Uuid,
        value: Option<PropertyValue>,
    ) -> impl Future<Output = Result<(), Self::Err>> + Send;

    /// Remove every property of an entity.
    fn delete_entity_properties_in(
        &self,
        transaction: &mut Self::Transaction,
        entity: &EntityReference,
    ) -> impl Future<Output = Result<(), Self::Err>> + Send;
}
