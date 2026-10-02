//! Transactional composition for the options of a database's select and tag
//! properties, inside the same transaction as its cells.

use models_properties::service::property_option::PropertyOptionValue;
use uuid::Uuid;

use super::database_cell_writer::DatabaseWriteTransaction;
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

/// Lets a composition root add, change and remove select options within an
/// owning use case's transaction.
pub trait DatabaseOptionWriter: DatabaseWriteTransaction {
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

    /// The ids among `ids` that already name an option of any definition.
    fn existing_option_ids_in(
        &self,
        transaction: &mut Self::Transaction,
        ids: &[Uuid],
    ) -> impl Future<Output = Result<Vec<Uuid>, Self::Err>> + Send;

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
}
