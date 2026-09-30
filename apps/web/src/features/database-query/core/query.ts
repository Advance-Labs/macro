import {
  isChartMode,
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
  actionSummary?: string;
  /** The model's chosen source; the query adapter verifies access before using it. */
  databaseId?: string;
  /** A verified complete schema, attached by the production query adapter. */
  source?: QuerySchema;
};
export type QueryResult = {
  columns: { name: string; entity_type: string | null }[];
  rows: (string | number | null)[][];
};
export type QueryAnswer = {
  results: QueryResult[];
  read_tables: string[];
  read_database_ids?: string[];
  read_versions: Record<string, number>;
  truncated_tables: string[];
  /** Source metadata checked against the query's actual read dependencies by its adapter. */
  source?: QuerySchema;
};

/** The write ledger succeeded, but the assistant could not finish its answer. */
export class QueryActionError extends Error {
  readonly actionSummary: string;
  constructor(actionSummary: string, message: string) {
    super(message);
    this.name = 'QueryActionError';
    this.actionSummary = actionSummary;
  }
}

/** The request may have committed tools before its final response was lost. */
export class QueryOutcomeUnknownError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'QueryOutcomeUnknownError';
  }
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

/**
 * Ready-made questions in the Macro Databases dialect: names are used as the
 * schema spells them, select items carry no aliases, and a result column is
 * named by the column's display name or the aggregate text (`COUNT(*)`).
 */
export function queryStarters(schema: QuerySchema) {
  const table = queryFocusTable(schema);
  if (!table) return [];
  const name = table.sqlName;
  const starters = [
    {
      label: 'Count records',
      prompt: `How many records are in ${table.name}?`,
      sql: `SELECT COUNT(*) FROM ${name}`,
    },
    {
      label: 'Preview records',
      prompt: `Show me the records in ${table.name}`,
      sql: `SELECT * FROM ${name} LIMIT 50`,
    },
  ];
  const group = table.columns.find(
    (column) => column.options.length && !column.multiple
  );
  if (group) {
    const column = group.sqlName;
    starters.push({
      label: `Count by ${group.name.toLowerCase()}`,
      prompt: `How many records have each ${group.name.toLowerCase()}?`,
      sql: `SELECT ${column}, COUNT(*) FROM ${name} GROUP BY ${column} ORDER BY COUNT(*) DESC`,
    });
  }
  return starters;
}

export function isScalarAnswer(answer: QueryAnswer | undefined): boolean {
  return (
    !!answer &&
    answer.results.length === 1 &&
    answer.results[0].columns.length === 1 &&
    answer.results[0].rows.length === 1
  );
}

export function formatQueryValue(
  value: string | number | null | undefined
): string {
  if (value === null || value === undefined) return '—';
  return typeof value === 'number'
    ? new Intl.NumberFormat(undefined, { maximumFractionDigits: 6 }).format(
        value
      )
    : value;
}

/** An early affordance, not a security boundary: the query endpoint enforces read-only SQL. */
export function looksLikeReadQuery(sql: string): boolean {
  const start = sql
    .replace(/^(?:\s|--[^\n]*(?:\n|$)|\/\*[\s\S]*?\*\/)+/, '')
    .trimStart();
  return /^SELECT\b/i.test(start);
}

export function queryErrorMessage(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  if (/no such table|unknown table/i.test(message))
    return 'This table is no longer available. Choose a database and update the question.';
  if (/no such column|unknown column/i.test(message))
    return 'A property in this question has changed. Try asking again with its current name.';
  if (/read.?only|not authorized|forbidden/i.test(message))
    return 'Questions can only read data you have access to. Edit records in the table or board.';
  if (/budget|timed out|timeout|too many/i.test(message))
    return 'This question needs less data. Try a narrower question or add a LIMIT in SQL.';
  if (/404|not found/i.test(message))
    return 'Live questions are not available on this server yet. Your question has been kept.';
  return message || 'We could not answer that question. Try again.';
}

export function parseQueryProposal(value: unknown): QueryProposal {
  if (typeof value !== 'object' || value === null)
    throw new Error('AI returned an incomplete question. Try again.');
  const record = value as Record<string, unknown>;
  if (record.answerable === false)
    throw new Error(
      typeof record.explanation === 'string'
        ? record.explanation
        : 'Try a question about the properties in this database.'
    );
  if (
    typeof record.sql !== 'string' ||
    !record.sql.trim() ||
    typeof record.explanation !== 'string' ||
    !record.explanation.trim()
  ) {
    throw new Error('AI returned an incomplete question. Try again.');
  }
  const sql = record.sql.trim().replace(/^```(?:sql)?\s*|\s*```$/g, '');
  if (!looksLikeReadQuery(sql))
    throw new Error(
      'Ask a question about your data. To make changes, use the table or board.'
    );
  const displayMode = record.displayMode;
  if (
    displayMode !== undefined &&
    displayMode !== 'scalar' &&
    displayMode !== 'table' &&
    !isChartMode(typeof displayMode === 'string' ? displayMode : undefined)
  )
    throw new Error('AI returned an unsupported answer display. Try again.');
  const chart =
    record.chart === undefined || record.chart === null
      ? undefined
      : parseQueryChart(record.chart);
  if (
    (record.chart != null && !chart) ||
    (isChartMode(displayMode as string | undefined) && !chart)
  )
    throw new Error('AI returned incomplete chart settings. Try again.');
  return {
    sql,
    explanation: record.explanation.trim(),
    ...(typeof record.title === 'string' && record.title.trim()
      ? { title: record.title.trim().slice(0, 100) }
      : {}),
    ...(typeof record.databaseId === 'string' && record.databaseId.trim()
      ? { databaseId: record.databaseId.trim() }
      : {}),
    ...(displayMode ? { displayMode: displayMode as QueryDisplayMode } : {}),
    ...(chart ? { chart } : {}),
  };
}
