//! Transactional composition for database rows' cells and their select
//! options, so a batch of row writes commits or rolls back as one.

use models_properties::EntityReference;
use models_properties::service::property_option::PropertyOptionValue;
use models_properties::service::property_value::PropertyValue;
use uuid::Uuid;

use super::model::UpdatePropertyOptionOutcome;

/// What an option update does to its colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorChange {
    /// Leave the colour as it is.
    Keep,
    /// Remove the colour.
    Clear,
    /// Store this hex colour.
    Set(String),
}

/// Lets a composition root write entity properties and select options within
/// an owning use case's transaction. The transaction stays opaque; adapters
/// choose its implementation.
pub trait DatabaseCellWriter: Send + Sync + 'static {
    /// Adapter-owned transaction handle.
    type Transaction: Send;
    /// Persistence or decoding error.
    type Err: std::error::Error + Send + Sync + 'static;

    /// Append options, under the ids given, after a definition's existing
    /// ones, holding the definition's lock so concurrent appends take
    /// distinct places. Each takes the palette colour of its position
    /// ([`crate::TagColor::for_position`]).
    fn add_options_in(
        &self,
        transaction: &mut Self::Transaction,
        property_definition_id: Uuid,
        options: &[(Uuid, PropertyOptionValue)],
    ) -> impl Future<Output = Result<(), Self::Err>> + Send;

    /// Change one option of a definition in place: its value when `value`
    /// is given, and its colour as `color` says. Every entity holding the
    /// option keeps it.
    fn update_option_in(
        &self,
        transaction: &mut Self::Transaction,
        property_definition_id: Uuid,
        option_id: Uuid,
        value: Option<PropertyOptionValue>,
        color: ColorChange,
    ) -> impl Future<Output = Result<UpdatePropertyOptionOutcome, Self::Err>> + Send;

    /// Remove one option of a definition and take its id out of every
    /// entity value holding it; a database row's cell left with no option is
    /// emptied. `false` when the definition has no such option.
    fn delete_option_in(
        &self,
        transaction: &mut Self::Transaction,
        property_definition_id: Uuid,
        option_id: Uuid,
    ) -> impl Future<Output = Result<bool, Self::Err>> + Send;

    /// Set one entity property, or with `None` clear its value.
    fn upsert_entity_property_in(
        &self,
        transaction: &mut Self::Transaction,
        entity: &EntityReference,
        property_definition_id: Uuid,
        value: Option<PropertyValue>,
    ) -> impl Future<Output = Result<(), Self::Err>> + Send;
}
