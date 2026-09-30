/**
 * Runs one `SELECT` through the SQL engine: the engine says what to fetch,
 * a `RowSource` fetches it, the driver feeds it back, until the engine has
 * the answer. The driver knows nothing about GraphQL; the source does (Soup
 * for database tables, the contacts query for `people`).
 */

import { match } from 'ts-pattern';
import type { Bin, Catalog, GqlQuery, Outcome, Page, Step } from './protocol';
import { type DatabaseSqlQuery, openDatabaseSqlQuery } from './wasm-module';

/** Where rows come from. Mirrors `database_sql::run::RowSource`. */
export interface RowSource {
  /**
   * One page of a `soup` or `people` query, from `cursor` (the start when
   * `null`), at most `limit` rows, with cells keyed by property definition.
   * `needs` names the keys the engine will read; a source may ignore it.
   */
  page: (
    query: GqlQuery,
    needs: string[],
    cursor: string | null,
    limit: number
  ) => Promise<Page>;
  /** The bins of a `groupSoup` query. */
  bins: (query: GqlQuery) => Promise<Bin[]>;
}

/** Opens the engine for one statement; the wasm module unless a test says otherwise. */
export type OpenEngine = (
  catalog: Catalog,
  sql: string
) => Promise<DatabaseSqlQuery>;

/** A statement the engine refused or a step it could not take. */
export class DatabaseSqlError extends Error {
  override name = 'DatabaseSqlError';
}

/** The engine throws its messages as strings; keep them readable as errors. */
function engineError(thrown: unknown): DatabaseSqlError {
  return new DatabaseSqlError(
    typeof thrown === 'string' ? thrown : String(thrown)
  );
}

async function nextStep(
  query: DatabaseSqlQuery,
  step: Exclude<Step, { step: 'done' }>,
  source: RowSource
): Promise<Step> {
  return match(step)
    .with({ step: 'fetch' }, async (request) => {
      const page = await source.page(
        request.query,
        request.needs,
        request.cursor,
        request.limit
      );
      try {
        return query.feed_page(request.id, page);
      } catch (thrown) {
        throw engineError(thrown);
      }
    })
    .with({ step: 'bins' }, async (request) => {
      const bins = await source.bins(request.query);
      try {
        return query.feed_bins(request.id, bins);
      } catch (thrown) {
        throw engineError(thrown);
      }
    })
    .exhaustive();
}

/**
 * Run a read-only statement to its outcome. The wasm engine loads on first
 * use; it is freed when the statement finishes or fails.
 */
export async function runDatabaseSql(
  catalog: Catalog,
  sql: string,
  {
    source,
    open = openDatabaseSqlQuery,
  }: { source: RowSource; open?: OpenEngine }
): Promise<Outcome> {
  let query: DatabaseSqlQuery;
  try {
    query = await open(catalog, sql);
  } catch (thrown) {
    throw engineError(thrown);
  }
  try {
    let step: Step;
    try {
      step = query.start();
    } catch (thrown) {
      throw engineError(thrown);
    }
    while (step.step !== 'done') {
      step = await nextStep(query, step, source);
    }
    const { step: _done, ...outcome } = step;
    return outcome;
  } finally {
    query.free();
  }
}
