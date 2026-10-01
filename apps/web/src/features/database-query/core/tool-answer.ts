/**
 * A QueryDatabase tool result as answers. The tool sends each result set as
 * JSON scalars rather than the engine's typed cells, so a column's kind is
 * read off the tool's entity type and its values; this is the one place
 * that reads that wire form.
 */

import type { AnswerColumn } from '@core/database-sql/answer';
import type { Cell, EntityKind } from '@core/database-sql/generated/types';
import type {
  QueryDatabaseResponse,
  ToolResultColumn,
} from '@service-cognition/generated/tools/types';
import { match, P } from 'ts-pattern';
import type { QueryAnswer } from './query';

/** The tool's entity types, as the server's result columns spell them. */
function entityKind(type: ToolResultColumn['entityType']): EntityKind | null {
  return match(type)
    .returnType<EntityKind | null>()
    .with('user', () => 'USER')
    .with('document', () => 'DOCUMENT')
    .with('channel', () => 'CHANNEL')
    .with('chat', () => 'CHAT')
    .with('project', () => 'PROJECT')
    .with('initiative', () => 'INITIATIVE')
    .with('email_thread', () => 'THREAD')
    .with('crm_company', () => 'COMPANY')
    .with('call', () => 'CALL_RECORD')
    .with('calendar_event', () => 'CALENDAR_EVENT')
    .with('database_row', () => 'DATABASE_ROW')
    .with(P.nullish, P.string, () => null)
    .exhaustive();
}

const INSTANT =
  /^\d{4}-\d{2}-\d{2}(?:T\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?(?:Z|[+-]\d{2}:?\d{2})?)?$/;

function isInstant(value: unknown): value is string {
  return (
    typeof value === 'string' &&
    INSTANT.test(value) &&
    !Number.isNaN(Date.parse(value))
  );
}

function toolColumn(column: ToolResultColumn, values: unknown[]): AnswerColumn {
  const target = entityKind(column.entityType);
  if (target)
    return {
      name: column.name,
      kind: 'entity',
      source: {
        markdown: false,
        options: [],
        tag: false,
        target,
        relatedTable: null,
      },
    };
  const present = values.filter(
    (value) => value !== null && value !== undefined
  );
  const every = (test: (value: unknown) => boolean) =>
    present.length > 0 && present.every(test);
  return {
    name: column.name,
    kind: every((value) => typeof value === 'number')
      ? 'number'
      : every((value) => typeof value === 'boolean')
        ? 'boolean'
        : every(isInstant)
          ? 'date'
          : 'text',
  };
}

/** A multi-valued id cell arrives as a JSON array, or its text. */
function ids(value: unknown): string[] {
  if (Array.isArray(value)) return value.map(String);
  if (typeof value === 'string' && value.startsWith('[')) {
    try {
      const parsed: unknown = JSON.parse(value);
      if (Array.isArray(parsed)) return parsed.map(String);
    } catch {
      // Not a list: the text is one id.
    }
  }
  return [String(value)];
}

function toolCell(value: unknown, column: AnswerColumn): Cell | null {
  if (value === null || value === undefined) return null;
  return match(column.kind)
    .returnType<Cell>()
    .with('entity', () => ({ type: 'entities', value: ids(value) }))
    .with('number', () => ({ type: 'number', value: Number(value) }))
    .with('boolean', () => ({ type: 'bool', value: value === true }))
    .with('date', () => ({ type: 'date', value: String(value) }))
    .with(P.union('text', 'select'), () => ({
      type: 'text',
      value: Array.isArray(value)
        ? value.map(String).join(', ')
        : String(value),
    }))
    .exhaustive();
}

/** A row-shaped result set leads with the rows' ids, which name rows rather than show. */
const ROW_ID_COLUMN = 'row_id';

export function toolAnswers(response: QueryDatabaseResponse): QueryAnswer[] {
  const readTables = response.readVersions.map((table) => table.tableId);
  return response.results.map((result) => {
    const rowShaped = result.columns[0]?.name === ROW_ID_COLUMN;
    const shown = rowShaped ? result.columns.slice(1) : result.columns;
    const values = result.rows.map((row) => (rowShaped ? row.slice(1) : row));
    const columns = shown.map((column, index) =>
      toolColumn(
        column,
        values.map((row) => row[index])
      )
    );
    return {
      columns,
      rows: values.map((row) =>
        columns.map((column, index) => toolCell(row[index], column))
      ),
      rowIds: rowShaped ? result.rows.map((row) => String(row[0])) : [],
      readTables,
      readDatabaseIds: [],
      truncatedTables: response.truncatedTables ?? [],
    };
  });
}
