/**
 * Typed surface of the generated wasm package (`database_sql`), loaded
 * dynamically so the repo type-checks without the generated artifacts and
 * the engine never slows app startup.
 *
 * Build the package with:
 *   just build-database-sql-wasm
 * which runs wasm-pack over crates/database_sql into
 * src/lib/core/database-sql/wasm/ (gitignored).
 */

import type {
  Bin,
  Catalog,
  OpResult,
  Page,
  Schema,
  Step,
} from './generated/types';

/**
 * One statement in flight. Mirrors `database_sql::wasm::Query`, whose
 * methods cross as untyped `JsValue`s: read the first step once, then feed
 * each request's answer back until `done`. Every method throws a string the
 * agent should read.
 */
export interface DatabaseSqlQuery {
  start: () => Step;
  feed_page: (requestId: number, page: Page) => Step;
  feed_bins: (requestId: number, bins: Bin[]) => Step;
  feed_ops: (requestId: number, results: OpResult[]) => Step;
  /** Releases the engine's wasm memory. */
  free: () => void;
}

interface DatabaseSqlWasmModule {
  default: (input?: { module_or_path?: unknown }) => Promise<unknown>;
  /** Compiles a statement. Throws a string when it does not compile. */
  Query: new (
    catalog: Catalog,
    sql: string
  ) => DatabaseSqlQuery;
  /** The catalog a statement run from `scope` sees. Throws a string. */
  buildCatalog: (schema: Schema, scope: string | undefined) => Catalog;
}

let modulePromise: Promise<DatabaseSqlWasmModule> | undefined;

/** Loads and initializes the wasm module exactly once per context. */
export function loadDatabaseSqlWasm(): Promise<DatabaseSqlWasmModule> {
  if (!modulePromise) {
    modulePromise = (async () => {
      const url = new URL('./wasm/database_sql.js', import.meta.url).href;
      const mod = (await import(
        /* @vite-ignore */ url
      )) as DatabaseSqlWasmModule;
      // Resolve the wasm binary explicitly: vite copies the generated JS as
      // an opaque asset, so its internal relative URL would 404 in
      // production. `new URL` is statically analyzable, so vite emits the
      // binary as an asset and rewrites it.
      const wasmUrl = new URL('./wasm/database_sql_bg.wasm', import.meta.url);
      await mod.default({ module_or_path: wasmUrl });
      return mod;
    })();
  }
  return modulePromise;
}

/** Compile `sql` against `catalog` in the wasm engine, loading it on first use. */
export async function openDatabaseSqlQuery(
  catalog: Catalog,
  sql: string
): Promise<DatabaseSqlQuery> {
  const { Query } = await loadDatabaseSqlWasm();
  return new Query(catalog, sql);
}

/** The catalog a statement run from `scope` sees, built by the engine. */
export async function buildDatabaseSqlCatalog(
  schema: Schema,
  scope?: string
): Promise<Catalog> {
  const { buildCatalog } = await loadDatabaseSqlWasm();
  return buildCatalog(schema, scope);
}
