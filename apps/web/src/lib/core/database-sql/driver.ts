/**
 * Runs one statement through the SQL engine: the engine says what to fetch
 * or write, a `RowSource` fetches it, an `OpsSink` applies the writes, and
 * the driver feeds each answer back until the engine has the outcome. The
 * driver knows nothing about GraphQL or HTTP; the source and the sink do.
 */

import type { DatabaseOpsError } from '@service-storage/databases';
import { err, errAsync, ok, okAsync, Result, ResultAsync } from 'neverthrow';
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

/** Why a statement has no outcome. */
export type DatabaseSqlFailure =
  /** The engine refused the statement or a step, in its own words. */
  | { kind: 'engine'; message: string }
  /** The row source could not read what the engine asked for. */
  | { kind: 'fetch'; message: string }
  /** The sink refused the statement's writes; none of them landed. */
  | { kind: 'ops'; error: DatabaseOpsError }
  /** The statement writes, and only reads run here. */
  | { kind: 'read-only' };

export type DatabaseSqlFetchFailure = Extract<
  DatabaseSqlFailure,
  { kind: 'fetch' }
>;

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
  ) => ResultAsync<Page, DatabaseSqlFetchFailure>;
  /** The bins of a `groupSoup` query. */
  bins: (query: GqlQuery) => ResultAsync<Bin[], DatabaseSqlFetchFailure>;
}

/** Where writes go. Mirrors `database_sql::run::OpsSink`. */
export interface OpsSink {
  /** Apply `ops` to `database` together; one result per op, in order. */
  apply: (
    database: string,
    ops: DatabaseOp[]
  ) => ResultAsync<OpResult[], DatabaseOpsError>;
}

/** Opens the engine for one statement; the wasm module unless a test says otherwise. */
export type OpenEngine = (
  catalog: Catalog,
  sql: string
) => Promise<DatabaseSqlQuery>;

/** The engine throws its messages as strings. */
export function engineFailure(thrown: unknown): DatabaseSqlFailure {
  return {
    kind: 'engine',
    message:
      typeof thrown === 'string'
        ? thrown
        : thrown instanceof Error
          ? thrown.message
          : String(thrown),
  };
}

function feed(next: () => Step): Result<Step, DatabaseSqlFailure> {
  return Result.fromThrowable(next, engineFailure)();
}

function nextStep(
  query: DatabaseSqlQuery,
  step: Exclude<Step, { step: 'done' }>,
  source: RowSource,
  ops: OpsSink | undefined
): ResultAsync<Step, DatabaseSqlFailure> {
  return match(step)
    .returnType<ResultAsync<Step, DatabaseSqlFailure>>()
    .with({ step: 'fetch' }, (request) =>
      source
        .page(request.query, request.needs, request.cursor, request.limit)
        .andThen((page) => feed(() => query.feed_page(request.id, page)))
    )
    .with({ step: 'bins' }, (request) =>
      source
        .bins(request.query)
        .andThen((bins) => feed(() => query.feed_bins(request.id, bins)))
    )
    .with({ step: 'ops' }, (request) =>
      ops
        ? ops
            .apply(request.database, request.ops)
            .mapErr((error): DatabaseSqlFailure => ({ kind: 'ops', error }))
            .andThen((results) =>
              feed(() => query.feed_ops(request.id, results))
            )
        : errAsync<Step, DatabaseSqlFailure>({ kind: 'read-only' })
    )
    .exhaustive();
}

async function steps(
  query: DatabaseSqlQuery,
  source: RowSource,
  ops: OpsSink | undefined
): Promise<Result<Outcome, DatabaseSqlFailure>> {
  let step = feed(() => query.start());
  while (step.isOk()) {
    const current = step.value;
    if (current.step === 'done') {
      const { step: _done, ...outcome } = current;
      return ok(outcome);
    }
    step = await nextStep(query, current, source, ops);
  }
  return err(step.error);
}

function drive(
  catalog: Catalog,
  sql: string,
  {
    source,
    ops,
    open = openDatabaseSqlQuery,
  }: { source: RowSource; ops?: OpsSink; open?: OpenEngine }
): ResultAsync<Outcome, DatabaseSqlFailure> {
  return ResultAsync.fromPromise(open(catalog, sql), engineFailure).andThen(
    (query) =>
      new ResultAsync(steps(query, source, ops).finally(() => query.free()))
  );
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
): ResultAsync<Outcome, DatabaseSqlFailure> {
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
): ResultAsync<Outcome, DatabaseSqlFailure> {
  return drive(catalog, sql, options);
}

const NO_ROWS: RowSource = {
  page: () => okAsync({ rows: [], next: null }),
  bins: () => okAsync([]),
};

/**
 * Compile and plan a statement against `catalog` without reading anything:
 * every fetch answers with no rows. A statement that writes, or answers with
 * no columns as a write does, is refused as read-only.
 */
export function checkReadStatement(
  catalog: Catalog,
  sql: string,
  options: { open?: OpenEngine } = {}
): ResultAsync<void, DatabaseSqlFailure> {
  return drive(catalog, sql, { source: NO_ROWS, ...options }).andThen(
    (outcome) =>
      outcome.columns.length > 0
        ? okAsync(undefined)
        : errAsync<void, DatabaseSqlFailure>({ kind: 'read-only' })
  );
}
