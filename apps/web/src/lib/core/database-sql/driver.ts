/**
 * Runs one statement through the SQL engine: the engine says what to fetch
 * or write, a `RowSource` fetches it, an `OpsSink` applies the writes, and
 * the driver feeds each answer back until the engine has the outcome. The
 * driver knows nothing about GraphQL or HTTP; the source and the sink do.
 */

import { match } from 'ts-pattern';
import type {
  Bin,
  Catalog,
  DatabaseOp,
  GqlQuery,
  OpResult,
  Outcome,
  Page,
  Step,
} from './generated/types';
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

/** Where writes go. Mirrors `database_sql::run::OpsSink`. */
export interface OpsSink {
  /** Apply `ops` to `database` together; one result per op, in order. */
  apply: (database: string, ops: DatabaseOp[]) => Promise<OpResult[]>;
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

function feed(next: () => Step): Step {
  try {
    return next();
  } catch (thrown) {
    throw engineError(thrown);
  }
}

async function nextStep(
  query: DatabaseSqlQuery,
  step: Exclude<Step, { step: 'done' }>,
  source: RowSource,
  ops: OpsSink | undefined
): Promise<Step> {
  return match(step)
    .with({ step: 'fetch' }, async (request) => {
      const page = await source.page(
        request.query,
        request.needs,
        request.cursor,
        request.limit
      );
      return feed(() => query.feed_page(request.id, page));
    })
    .with({ step: 'bins' }, async (request) => {
      const bins = await source.bins(request.query);
      return feed(() => query.feed_bins(request.id, bins));
    })
    .with({ step: 'ops' }, async (request) => {
      if (!ops)
        throw new DatabaseSqlError(
          'This statement changes data, and only reads are run here.'
        );
      const results = await ops.apply(request.database, request.ops);
      return feed(() => query.feed_ops(request.id, results));
    })
    .exhaustive();
}

async function drive(
  catalog: Catalog,
  sql: string,
  {
    source,
    ops,
    open = openDatabaseSqlQuery,
  }: { source: RowSource; ops?: OpsSink; open?: OpenEngine }
): Promise<Outcome> {
  let query: DatabaseSqlQuery;
  try {
    query = await open(catalog, sql);
  } catch (thrown) {
    throw engineError(thrown);
  }
  try {
    let step = feed(() => query.start());
    while (step.step !== 'done') {
      step = await nextStep(query, step, source, ops);
    }
    const { step: _done, ...outcome } = step;
    return outcome;
  } finally {
    query.free();
  }
}

/**
 * Run a read-only statement to its outcome; a write is refused. The wasm
 * engine loads on first use; it is freed when the statement finishes or
 * fails.
 */
export function runDatabaseSql(
  catalog: Catalog,
  sql: string,
  options: { source: RowSource; open?: OpenEngine }
): Promise<Outcome> {
  return drive(catalog, sql, options);
}

/**
 * Run any statement to its outcome: reads through `source`, and a write's
 * ops, once the engine has found the rows it changes, through `ops`.
 */
export function runDatabaseSqlStatement(
  catalog: Catalog,
  sql: string,
  options: { source: RowSource; ops: OpsSink; open?: OpenEngine }
): Promise<Outcome> {
  return drive(catalog, sql, options);
}
