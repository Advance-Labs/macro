//! Assembling the databases service from its Postgres adapters.
//!
//! Every host — the HTTP service, the MCP service, the AI-tool contexts —
//! needs the *same* adapters behind the service, and a host that wires a
//! different set is a host whose SQL behaves differently from everyone else's.
//! So the wiring lives here once and the roots call it, rather than each
//! repeating the constructor. That makes this the crate's composition root:
//! the only place it constructs other crates' outbound adapters.
//!
//! The only things a host chooses are where table-changed liveness pings go
//! (a process with gateway credentials publishes, one without drops them) and
//! which broker carries the durable `macro.databases` events.

use entity_access::domain::service::EntityAccessServiceImpl;
use entity_access::outbound::PgAccessRepository;
use macro_event_broker::MacroEventBroker;
use properties::outbound::properties_pg_repo::PropertiesPgRepo;
use sqlx::PgPool;

use crate::domain::ports::TableEventPublisher;
use crate::domain::service::DatabasesServiceImpl;
use crate::outbound::entity_access_directory::EntityAccessDirectory;
use crate::outbound::pg_cell_store::PgCellStore;
use crate::outbound::pg_databases_repo::PgDatabasesRepo;
use crate::outbound::pg_definition_store::PgDefinitionStore;

/// The service as every host builds it.
pub type PgDatabasesService<Events, Broker> = DatabasesServiceImpl<
    PgDatabasesRepo,
    PgDefinitionStore<PropertiesPgRepo>,
    PgCellStore<PropertiesPgRepo>,
    Events,
    EntityAccessDirectory<EntityAccessServiceImpl<PgAccessRepository>>,
    Broker,
>;

/// Build the databases service over `pool`, publishing table changes through
/// `events` and domain events through `broker`.
pub fn build_service<Events: TableEventPublisher, Broker: MacroEventBroker>(
    pool: PgPool,
    events: Events,
    broker: Broker,
) -> PgDatabasesService<Events, Broker> {
    DatabasesServiceImpl::new(
        PgDatabasesRepo::new(pool.clone()),
        PgDefinitionStore::new(PropertiesPgRepo::new(pool.clone())),
        PgCellStore::new(PropertiesPgRepo::new(pool.clone())),
        events,
        EntityAccessDirectory::new(EntityAccessServiceImpl::new(PgAccessRepository::new(pool))),
        broker,
    )
}
