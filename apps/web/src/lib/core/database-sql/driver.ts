/** Feeds the engine what a `RowSource` reads until it has the outcome; a write is refused. */

import { err, errAsync, ok, okAsync, Result, ResultAsync } from 'neverthrow';
import { match } from 'ts-pattern';
import type {
  Bin,
  Catalog,
  DatabaseView,
  EngineError,
  GqlQuery,
  Outcome,
  Page,
  RunError,
  Step,
} from './generated/types';
import {
  type DatabaseSqlQuery,
  openDatabaseSqlQuery,
  openDatabaseViewQuery,
} from './wasm-module';

/** Why a statement has no outcome. */
export type DatabaseSqlFailure =
  /** The engine refused the statement or a step; `message` is its words for an agent. */
  | { kind: 'engine'; error: RunError; message: string }
  /** The engine could not be loaded or did not answer as an engine does. */
  | { kind: 'crash'; message: string }
  /** The row source could not read what the engine asked for. */
  | { kind: 'fetch'; message: string }
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

/** Opens the engine for one statement; the wasm module unless a test says otherwise. */
export type OpenEngine = (
  catalog: Catalog,
  sql: string
) => Promise<DatabaseSqlQuery>;

/** Opens the engine for one view's rows; the wasm module unless a test says otherwise. */
export type OpenView = (
  catalog: Catalog,
  view: DatabaseView
) => Promise<DatabaseSqlQuery>;

function isEngineError(thrown: unknown): thrown is EngineError {
  return (
    !!thrown &&
    typeof thrown === 'object' &&
    'error' in thrown &&
    'message' in thrown &&
    typeof thrown.message === 'string'
  );
}

/** The engine throws an `EngineError`; anything else is a crash. */
export function engineFailure(thrown: unknown): DatabaseSqlFailure {
  if (isEngineError(thrown))
    return { kind: 'engine', error: thrown.error, message: thrown.message };
  return {
    kind: 'crash',
    message: thrown instanceof Error ? thrown.message : String(thrown),
  };
}

function feed(next: () => Step): Result<Step, DatabaseSqlFailure> {
  return Result.fromThrowable(next, engineFailure)();
}

function nextStep(
  query: DatabaseSqlQuery,
  step: Exclude<Step, { step: 'done' }>,
  source: RowSource
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
    .with({ step: 'ops' }, () =>
      errAsync<Step, DatabaseSqlFailure>({ kind: 'read-only' })
    )
    .exhaustive();
}

async function steps(
  query: DatabaseSqlQuery,
  source: RowSource
): Promise<Result<Outcome, DatabaseSqlFailure>> {
  let step = feed(() => query.start());
  while (step.isOk()) {
    const current = step.value;
    if (current.step === 'done') {
      const { step: _done, ...outcome } = current;
      return ok(outcome);
    }
    step = await nextStep(query, current, source);
  }
  return err(step.error);
}

function drive(
  opened: Promise<DatabaseSqlQuery>,
  source: RowSource
): ResultAsync<Outcome, DatabaseSqlFailure> {
  return ResultAsync.fromPromise(opened, engineFailure).andThen(
    (query) => new ResultAsync(steps(query, source).finally(() => query.free()))
  );
}

/** Run a read-only statement to its outcome; a write is refused. */
export function runDatabaseSql(
  catalog: Catalog,
  sql: string,
  {
    source,
    open = openDatabaseSqlQuery,
  }: { source: RowSource; open?: OpenEngine }
): ResultAsync<Outcome, DatabaseSqlFailure> {
  return drive(open(catalog, sql), source);
}

/** Read the rows a view shows, as its compiled query finds them. */
export function runDatabaseView(
  catalog: Catalog,
  view: DatabaseView,
  {
    source,
    open = openDatabaseViewQuery,
  }: { source: RowSource; open?: OpenView }
): ResultAsync<Outcome, DatabaseSqlFailure> {
  return drive(open(catalog, view), source);
}

const NO_ROWS: RowSource = {
  page: () => okAsync({ rows: [], next: null }),
  bins: () => okAsync([]),
};

/** Compile and plan a statement with every fetch answering no rows; a write is refused. */
export function checkReadStatement(
  catalog: Catalog,
  sql: string,
  { open = openDatabaseSqlQuery }: { open?: OpenEngine } = {}
): ResultAsync<void, DatabaseSqlFailure> {
  return drive(open(catalog, sql), NO_ROWS).andThen((outcome) =>
    outcome.columns.length > 0
      ? okAsync(undefined)
      : errAsync<void, DatabaseSqlFailure>({ kind: 'read-only' })
  );
}
