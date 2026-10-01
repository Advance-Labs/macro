# Database backend

Databases contain tables of typed rows. A row is a `DATABASE_ROW` entity whose
cells are its entity properties, and column placements bind to the existing
property definitions and value types. There is no second store.

## Reads and writes

1. Rows are read as Soup items, scoped to the databases the caller can reach as
   `entity_access` answers it.
2. Every write to a database's schema or data is one batch of typed ops
   (`models_databases::DatabaseOp`, `POST /databases/{id}/ops`,
   `DatabasesService::apply_ops`): tables (create, rename, delete, reorder),
   columns (create, rename, delete, reorder, change type), select options,
   rows, views and a board's card moves. Ops are checked against the receipt's
   database and apply in order in one transaction, all or nothing; any refusal
   writes nothing. Lists are ordered by fractional keys
   (`models_databases::position`), stored `COLLATE "C"`.
3. Later ops of a batch see what earlier ops did. `create_table`,
   `create_column` and new options carry ids the client mints (UUIDv7), so a
   later op can name a table, column or option the same batch created: create
   a table, give it columns, then insert rows into them, in one request. An id
   that already names something in the database, or is minted twice in one
   batch, refuses the batch with a 400 whose `taken` is
   `{kind: "table" | "column" | "option", id}`; a retry of a committed batch
   lands there instead of writing twice. Rows keep server-minted ids, answered
   in the `insert_rows` result.
4. The optional request-level `baseVersions` names tables and the version the
   caller read; a table at another version answers 409 and writes nothing. A
   column type change follows one cast rule for the type menu, the agent tool
   and `ALTER COLUMN`.
5. Successful commits publish their versions and change notifications.

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
| Rows, cells, imports, and column definitions | `src/outbound/pg_databases_repo.rs`, `pg_cell_store.rs`, `pg_definition_store.rs` |
| Table, column and cell ops | `src/domain/service/ops/tables.rs`, `columns.rs`, `cells.rs`, `src/outbound/pg_databases_repo/schema.rs` |
| Column casts and inference | `src/domain/service/casts.rs`, `column_types.rs`, `infer_column_type.rs` |
| Typed views and a board's card places | `models_databases::views`, `src/domain/service/ops/views.rs`, `src/outbound/pg_databases_repo/views.rs` |
| Saved queries, sharing, imports, starter data | Corresponding modules under `src/domain/` |
| HTTP transport | `src/inbound/axum_router.rs`, `starter_router.rs` |
| Service construction and notifications | `src/outbound/build.rs`, `gateway_event_publisher.rs` |

Authorization and business policy live in domain services. HTTP adapters obtain
typed access receipts and pass requests inward. Persistence adapters implement
domain ports, including transaction and locking requirements.

The document storage service mounts the routes. Writes inside a database go
through `POST /databases/{id}/ops`; the other routes are the database list and
detail, create, starter, CSV import, a board's card positions, a column's casts
and type inference, awareness, permissions and saved queries.

## Validation

Run from the repository root inside Nix, with a migrated local PostgreSQL database
and `SQLX_OFFLINE` unset:

```sh
cargo test -p databases --features postgres,inbound,ai_tools,gateway,entity_mutation
```

The suite covers permission scoping, typed round trips, relations, safe casts,
rollback, stale/concurrent writes, sharing, typed views and card moves, and
retry-safe import/starter provisioning. SQLx tests create isolated databases
using the repository migrator.
