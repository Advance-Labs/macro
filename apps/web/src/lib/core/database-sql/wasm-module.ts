/**
 * Typed surface of the generated wasm package (`database_sql`), loaded
 * dynamically so the repo type-checks without the generated artifacts.
 *
 * Build the package with:
 *   just build-database-sql-wasm
 * which runs wasm-pack over crates/database_sql into
 * src/lib/core/database-sql/wasm/ (gitignored).
 */

import type { Bin, Catalog, Page, Step } from './protocol';

/**
 * One `SELECT` in flight. Mirrors `database_sql::wasm::Query`: read the
 * first step once, then feed each request's answer back until `done`.
 * Every method throws a string the agent should read.
 */
export interface Query {
  start: () => Step;
  feed_page: (requestId: number, page: Page) => Step;
  feed_bins: (requestId: number, bins: Bin[]) => Step;
  /** Releases the engine's wasm memory. */
  free: () => void;
}

interface DatabaseSqlWasmModule {
  default: (input?: { module_or_path?: unknown }) => Promise<unknown>;
  /** Compiles a statement. Throws a string when it does not compile. */
  Query: new (
    catalog: Catalog,
    sql: string
  ) => Query;
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
