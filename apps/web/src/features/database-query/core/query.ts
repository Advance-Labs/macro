import { isFeatureEnabled, showDatabaseSql } from '@core/constant/featureFlags';
import type { DatabaseSqlAnswer } from '@core/database-sql/answer';
import type { DatabaseSqlFailure } from '@core/database-sql/driver';
import type { RunError } from '@core/database-sql/generated/types';
import { type ResultError, ThrownResultError } from '@core/util/result';
import { err, ok, type Result } from 'neverthrow';
import { match, P } from 'ts-pattern';
import {
  isChartMode,
  isDisplayMode,
  parseQueryChart,
  type QueryChartConfig,
  type QueryDisplayMode,
} from './query-chart';

export type QueryDefinition = {
  databaseId?: string;
  /** Default subject for new questions; SQL remains the saved answer's source. */
  tableId?: string;
  sql: string;
  prompt: string;
  title?: string;
  displayMode: QueryDisplayMode;
  chart?: QueryChartConfig;
};

/**
 * What a document stores: a pointer to an immutable saved query plus its
 * presentation. An empty `queryId` is a draft that has not been saved yet.
 */
export type SavedQuestion = Omit<QueryDefinition, 'sql'> & { queryId: string };

export type QuerySchema = {
  /** Undefined lets a document question discover its source automatically. */
  databaseId?: string;
  name: string;
  /** The current table is the default subject; the complete schema remains available. */
  focusTableId?: string;
  tables: {
    id: string;
    name: string;
    sqlName: string;
    primaryKey?: string;
    columns: {
      name: string;
      sqlName: string;
      type: string;
      options: string[];
      multiple: boolean;
      relation?: {
        databaseId: string;
        tableId: string;
        writable: boolean;
      };
    }[];
  }[];
};

export type QueryProposal = {
  title?: string;
  sql: string;
  explanation: string;
  displayMode?: QueryDisplayMode;
  chart?: QueryChartConfig;
  /** The model's chosen source; the query adapter verifies access before using it. */
  databaseId?: string;
  /** A verified complete schema, attached by the production query adapter. */
  source?: QuerySchema;
};
export type QueryAnswer = DatabaseSqlAnswer & {
  /** Source metadata checked against the query's actual read dependencies by its adapter. */
  source?: QuerySchema;
};

/** Why a question has no answer. */
export type QueryFailure =
  | DatabaseSqlFailure
  /** Its saved query could not be loaded or saved. */
  | { kind: 'question'; error: ResultError }
  /** The databases it reads could not be loaded. */
  | { kind: 'databases'; error: ResultError }
  /** The assistant could not turn the question into a query, in its words. */
  | { kind: 'generation'; message: string }
  /** The question names a table the source no longer has. */
  | { kind: 'table-unavailable' }
  /** The question needs a database other than the one chosen. */
  | { kind: 'other-database' }
  /** The answer's source could not be matched to a database the viewer reads. */
  | { kind: 'unverified-source' };

/** A service's failure: the first of the errors it reported. */
export function serviceError(errors: readonly ResultError[]): ResultError {
  return (
    errors[0] ?? { code: 'UNKNOWN_ERROR', message: 'The service did not say.' }
  );
}

/** A query read's failure, thrown at the TanStack boundary with its codes. */
export function thrownServiceError(thrown: unknown): ResultError {
  return thrown instanceof ThrownResultError
    ? serviceError(thrown.errors)
    : {
        code: 'UNKNOWN_ERROR',
        message: thrown instanceof Error ? thrown.message : String(thrown),
      };
}

export function generationFailure(message: string): QueryFailure {
  return { kind: 'generation', message };
}

/**
 * A schema name as it appears in the SQL text — `sqlName` arrives from the
 * API already double-quoted, e.g. `"Guest List"` — reduced to the plain name
 * completion offers and results carry.
 */
export function unquoteIdentifier(sqlName: string): string {
  // A table's SQL name is qualified (`"Database"."Table"`); the last quoted
  // segment is the table's own name.
  const own = sqlName.endsWith('"') ? lastQuotedSegment(sqlName) : sqlName;
  return own.startsWith('"') && own.endsWith('"')
    ? own.slice(1, -1).replaceAll('""', '"')
    : own;
}

function lastQuotedSegment(sqlName: string): string {
  let end = sqlName.length - 1;
  let start = end - 1;
  while (start >= 0) {
    if (sqlName[start] === '"') {
      if (sqlName[start - 1] === '"') {
        start -= 2;
        continue;
      }
      break;
    }
    start -= 1;
  }
  return start >= 0 ? sqlName.slice(start, end + 1) : sqlName;
}

export function queryFocusTable(schema: QuerySchema) {
  return schema.focusTableId
    ? schema.tables.find((table) => table.id === schema.focusTableId)
    : schema.tables[0];
}

export function isScalarAnswer(answer: QueryAnswer | undefined): boolean {
  return !!answer && answer.columns.length === 1 && answer.rows.length === 1;
}

export function formatQueryValue(value: number | null | undefined): string {
  if (value === null || value === undefined) return '—';
  return new Intl.NumberFormat(undefined, { maximumFractionDigits: 6 }).format(
    value
  );
}

/** An early affordance, not a security boundary: the query endpoint enforces read-only SQL. */
export function looksLikeReadQuery(sql: string): boolean {
  const start = sql
    .replace(/^(?:\s|--[^\n]*(?:\n|$)|\/\*[\s\S]*?\*\/)+/, '')
    .trimStart();
  return /^SELECT\b/i.test(start);
}

const UNCOMPUTED = "This answer couldn't be computed";
const TRY_AGAIN = 'Something went wrong reaching your data. Try again.';
const OFFLINE = 'Your data could not be reached. Check your connection.';

/** An engine refusal in the reader's terms rather than the statement's. */
function engineErrorMessage(error: RunError): string {
  return match(error)
    .returnType<string>()
    .with(
      { stage: 'resolve', kind: 'unknownColumn' },
      ({ name }) => `${UNCOMPUTED}: the column ${name} no longer exists.`
    )
    .with(
      { stage: 'resolve', kind: 'unknownTable' },
      ({ name }) => `${UNCOMPUTED}: the table ${name} no longer exists.`
    )
    .with(
      { stage: 'resolve', kind: 'ambiguousTable' },
      ({ name }) =>
        `${UNCOMPUTED}: more than one database has a table named ${name}.`
    )
    .with(
      { stage: 'resolve', kind: 'unknownOption' },
      ({ label, column }) =>
        `${UNCOMPUTED}: ${label} is not an option of ${column}.`
    )
    .with(
      { stage: 'resolve', kind: 'typeMismatch' },
      ({ column, expected }) =>
        `${UNCOMPUTED}: ${column} holds ${expected} values, which don't fit this question.`
    )
    .with(
      {
        stage: 'resolve',
        kind: P.union(
          'operatorNotSupported',
          'hasOnSingleValued',
          'equalityOnMultiValued',
          'compareToNull',
          'aggregateNotSupported',
          'listOnSingleValued',
          'listInComparison'
        ),
      },
      ({ column }) => `${UNCOMPUTED}: ${column} can't be used that way.`
    )
    .with(
      { stage: 'resolve', kind: 'readOnlyTable' },
      () =>
        'Questions can only read data you have access to. Edit records in the table or board.'
    )
    .with(
      { stage: 'tooManyRows' },
      () => 'This request matches too many records. Try a narrower request.'
    )
    .otherwise(() => `${UNCOMPUTED}. Try asking again.`);
}

/**
 * What to tell a person when a question fails. With SQL hidden
 * ({@link showDatabaseSql}) the engine's and compiler's own words, which
 * quote the statement, become a plain line; {@link queryFailureDetail}
 * keeps them for the agent and the SQL-visible UI.
 */
export function queryErrorMessage(failure: QueryFailure): string {
  const showSql = isFeatureEnabled(showDatabaseSql);
  return match(failure)
    .returnType<string>()
    .with({ kind: 'engine' }, ({ error, message }) =>
      showSql ? message : engineErrorMessage(error)
    )
    .with({ kind: 'crash' }, () => `${UNCOMPUTED}. Try asking again.`)
    .with({ kind: 'fetch' }, () => OFFLINE)
    .with({ kind: 'read-only' }, () =>
      showSql
        ? 'Questions only read your data. Start with SELECT, or ask a question above.'
        : 'Questions only read your data. Ask a question about it above.'
    )
    .with({ kind: 'question' }, ({ error }) =>
      match(error.code)
        .with('NOT_FOUND', () => 'This saved question no longer exists.')
        .with('INVALID_QUERY', () =>
          showSql ? error.message : `${UNCOMPUTED}. Try asking again.`
        )
        .with(
          'READ_ONLY',
          () =>
            'Questions can only read data you have access to. Edit records in the table or board.'
        )
        .with('BUDGET_EXCEEDED', () =>
          showSql
            ? 'This question needs less data. Try a narrower question or add a LIMIT in SQL.'
            : 'This question needs less data. Try a narrower question.'
        )
        .with('NETWORK_ERROR', () => OFFLINE)
        .otherwise(() => TRY_AGAIN)
    )
    .with({ kind: 'databases' }, ({ error }) =>
      match(error.code)
        .with(
          P.union('NOT_FOUND', 'FORBIDDEN', 'GONE'),
          () =>
            'This table is no longer available. Choose a database and update the question.'
        )
        .with('NETWORK_ERROR', () => OFFLINE)
        .otherwise(() => TRY_AGAIN)
    )
    .with({ kind: 'generation' }, ({ message }) => message)
    .with(
      { kind: 'table-unavailable' },
      () => 'Choose an available table before asking this question.'
    )
    .with(
      { kind: 'other-database' },
      () =>
        'This answer reads another database. Choose Automatic or change the source, then ask again.'
    )
    .with(
      { kind: 'unverified-source' },
      () => 'The answer’s source could not be verified. Try again.'
    )
    .exhaustive();
}

/** The failure in the engine's or the service's own words, when it has them. */
export function queryFailureDetail(failure: QueryFailure): string | undefined {
  return match(failure)
    .with(
      { kind: P.union('engine', 'crash', 'fetch') },
      ({ message }) => message
    )
    .with(
      { kind: P.union('question', 'databases') },
      ({ error }) => error.message
    )
    .otherwise(() => undefined);
}

export function parseQueryProposal(
  value: unknown
): Result<QueryProposal, QueryFailure> {
  if (typeof value !== 'object' || value === null)
    return err(
      generationFailure('AI returned an incomplete question. Try again.')
    );
  const record = value as Record<string, unknown>;
  if (record.answerable === false)
    return err(
      generationFailure(
        typeof record.explanation === 'string'
          ? record.explanation
          : 'Try a question about the properties in this database.'
      )
    );
  if (
    typeof record.sql !== 'string' ||
    !record.sql.trim() ||
    typeof record.explanation !== 'string' ||
    !record.explanation.trim()
  ) {
    return err(
      generationFailure('AI returned an incomplete question. Try again.')
    );
  }
  const sql = record.sql.trim().replace(/^```(?:sql)?\s*|\s*```$/g, '');
  if (!looksLikeReadQuery(sql))
    return err(
      generationFailure(
        'Ask a question about your data. To make changes, use the table or board.'
      )
    );
  const displayMode = record.displayMode;
  if (displayMode !== undefined && !isDisplayMode(displayMode))
    return err(
      generationFailure('AI returned an unsupported answer display. Try again.')
    );
  const chart =
    record.chart === undefined || record.chart === null
      ? undefined
      : parseQueryChart(record.chart);
  if ((record.chart != null && !chart) || (isChartMode(displayMode) && !chart))
    return err(
      generationFailure('AI returned incomplete chart settings. Try again.')
    );
  return ok({
    sql,
    explanation: record.explanation.trim(),
    ...(typeof record.title === 'string' && record.title.trim()
      ? { title: record.title.trim().slice(0, 100) }
      : {}),
    ...(typeof record.databaseId === 'string' && record.databaseId.trim()
      ? { databaseId: record.databaseId.trim() }
      : {}),
    ...(displayMode ? { displayMode } : {}),
    ...(chart ? { chart } : {}),
  });
}
