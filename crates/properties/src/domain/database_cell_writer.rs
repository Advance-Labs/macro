//! Transactional composition for database rows' cells, so a batch of writes
//! commits or rolls back as one.

use models_properties::EntityReference;
use models_properties::service::property_value::PropertyValue;
use uuid::Uuid;

/// The transaction a database's writes share. The option and cell writers
/// both run inside it, so one use case can call either within the same
/// commit. It stays opaque; adapters choose its implementation.
pub trait DatabaseWriteTransaction: Send + Sync + 'static {
    /// Adapter-owned transaction handle.
    type Transaction: Send;
    /// Persistence or decoding error.
    type Err: std::error::Error + Send + Sync + 'static;
}

/// Lets a composition root write entity properties within an owning use
/// case's transaction.
pub trait DatabaseCellWriter: DatabaseWriteTransaction {
    /// Set one entity property, or with `None` clear its value.
    fn upsert_entity_property_in(
        &self,
        transaction: &mut Self::Transaction,
        entity: &EntityReference,
        property_definition_id: Uuid,
        value: Option<PropertyValue>,
    ) -> impl Future<Output = Result<(), Self::Err>> + Send;
}
