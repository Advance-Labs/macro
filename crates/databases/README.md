# Database backend

Databases contain tables of typed rows. A row is a `DATABASE_ROW` entity whose
cells are its entity properties, and column placements bind to the existing
property definitions and value types. There is no second store.

## Reads and writes

1. Rows are read as Soup items, scoped to the databases the caller can reach as
   `entity_access` answers it.
2. Every data write is a batch of typed ops (`models_databases::DatabaseOp`,
   `POST /databases/{id}/ops`), checked against the receipt's database and
   applied in one transaction, all or nothing.
3. Schema changes are structured calls; a column type change follows one cast
   rule for the type menu, the agent tool and `ALTER COLUMN`.
4. Successful commits publish their versions and change notifications.

SQL lives outside this crate. The browser compiles statements with the
`database_sql` engine and posts the ops it emits; agents run the same engine
through the `databases_sql` adapter, which reads through Soup and writes
through `apply_ops`.

## Review map

| Concern | Implementation |
| --- | --- |
| Domain contracts and orchestration | `src/domain/models.rs`, `ports.rs`, `service.rs` |
| Catalog entries | `src/domain/catalog.rs` |
| Typed ops | `src/domain/service/ops.rs`, `models_databases` |
| Rows, cells, and column definitions | `src/outbound/pg_databases_repo.rs`, `pg_cell_store.rs`, `pg_definition_store.rs` |
| Column casts and inference | `src/domain/service/columns.rs`, `column_types.rs`, `infer_column_type.rs` |
| Saved queries, views, sharing, imports, starter data | Corresponding modules under `src/domain/` |
| HTTP transport | `src/inbound/axum_router.rs`, `starter_router.rs` |
| Service construction and notifications | `src/outbound/build.rs`, `gateway_event_publisher.rs` |

Authorization and business policy live in domain services. HTTP adapters obtain
typed access receipts and pass requests inward. Persistence adapters implement
domain ports, including transaction and locking requirements.

The document storage service exposes the schema operations, `/databases/{id}/ops`
and the saved-query routes.

## Validation

Run from the repository root inside Nix, with a migrated local PostgreSQL database
and `SQLX_OFFLINE` unset:

```sh
cargo test -p databases --features postgres,inbound,ai_tools,gateway,entity_mutation
```

The suite covers permission scoping, typed round trips, relations, safe casts,
rollback, stale/concurrent writes, sharing, saved views, and retry-safe
import/starter provisioning. SQLx tests
create isolated databases using the repository migrator.
