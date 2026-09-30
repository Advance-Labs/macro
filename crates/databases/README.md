# Database backend

Databases contain tables of typed rows. A row is a `DATABASE_ROW` entity whose
cells are its entity properties, and column placements bind to the existing
property definitions and value types. There is no second store: user SQL is
compiled by the `database_sql` crate and answered from the same rows.

## Query and mutation flow

1. The domain service builds the caller's catalog from the databases they can
   reach, as `entity_access` answers it. An unreadable table does not exist to
   the statement, and a table without edit access is read-only.
2. `database_sql` parses the statement against that catalog: names resolve,
   literals are typed, and the filter is split into what Soup evaluates and
   what is folded afterwards.
3. Reads load the referenced tables' rows and cells within row and time
   budgets and fold the answer. Read-only entry points refuse writes.
4. Writes go through the row and cell stores one row at a time, guarded by
   table versions. Stale versions, changed bindings, and trashed parents
   reject the write.
5. Successful commits publish their versions and change notifications.

## Review map

| Concern | Implementation |
| --- | --- |
| Domain contracts and orchestration | `src/domain/models.rs`, `ports.rs`, `service.rs` |
| Catalog and the SQL pipeline | `src/domain/catalog.rs`, `src/domain/service/query.rs`, the `database_sql` crate |
| Rows, cells, and column definitions | `src/outbound/pg_databases_repo.rs`, `pg_cell_store.rs`, `pg_definition_store.rs` |
| Column casts and inference | `src/domain/service/columns.rs`, `column_types.rs`, `infer_column_type.rs` |
| Saved queries, views, sharing, imports, starter data | Corresponding modules under `src/domain/` |
| HTTP transport | `src/inbound/axum_router.rs`, `starter_router.rs` |
| Service construction and notifications | `src/outbound/build.rs`, `gateway_event_publisher.rs` |

Authorization and business policy live in domain services. HTTP adapters obtain
typed access receipts and pass requests inward. Persistence adapters implement
domain ports, including transaction and locking requirements.

The document storage service exposes schema operations and the SQL-first
`/databases/query` and `/databases/exec` endpoints. The SDK provides database,
table, and column handles, queries, mutations, imports, and snapshot downloads.

## Validation

Run from the repository root inside Nix, with a migrated local PostgreSQL database
and `SQLX_OFFLINE` unset:

```sh
cargo test -p databases --features postgres,inbound,ai_tools,gateway,entity_mutation
```

The suite covers permission scoping, read-only enforcement, query budgets,
typed round trips, relations, safe casts, rollback, stale/concurrent writes,
sharing, saved views, and retry-safe import/starter provisioning. SQLx tests
create isolated databases using the repository migrator.
