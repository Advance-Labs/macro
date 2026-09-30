import { fromCellDate } from '@app/features/block-database/core/cell-date';
import type { DatabaseEntityType } from '@app/features/block-database/core/column-inference';
import type { DatabaseViewColumn } from '@app/features/block-database/core/database-view';
import { formatCellValue } from '@app/features/block-database/core/table';
import { formatTime } from '@core/util/date';
import { markdownToPlainText } from '@macro-inc/lexical-core/utils/parsers';
import { formatDate } from '@property/utils/formatting';
import { match, P } from 'ts-pattern';
import { formatQueryValue, type QueryResult } from './query';

export type ResultColumn = QueryResult['columns'][number];
export type ResultValue = QueryResult['rows'][number][number];

/** What one result value shows, drawn with the database grid's own pieces. */
export type ResultCell =
  | { kind: 'empty' }
  | { kind: 'text'; text: string }
  | { kind: 'markdown'; markdown: string }
  | { kind: 'boolean'; checked: boolean }
  | { kind: 'options'; labels: string[] }
  | { kind: 'mentions'; entityType: DatabaseEntityType; ids: string[] };

const SELECT_TYPES = ['SELECT_STRING', 'SELECT_NUMBER', 'TAG'];

/**
 * A result value as the grid would show it. The column the value was read
 * from, when the answer traces it to one, decides; otherwise the engine's
 * entity type and the value's own shape do.
 */
export function resultCell(
  value: ResultValue,
  column: ResultColumn,
  databaseColumn?: DatabaseViewColumn
): ResultCell {
  if (value === null || value === '') return { kind: 'empty' };
  if (databaseColumn) return columnCell(value, databaseColumn);
  const entityType = resultEntityType(column.entity_type);
  if (entityType === 'DATABASE_ROW') return linkedRecords(value);
  if (entityType)
    return { kind: 'mentions', entityType, ids: listedValues(value) };
  if (typeof value === 'number')
    return { kind: 'text', text: formatQueryValue(value) };
  return { kind: 'text', text: formatDateValue(value) ?? value };
}

function columnCell(
  value: string | number,
  column: DatabaseViewColumn
): ResultCell {
  if (column.relation) return linkedRecords(value);
  if (column.dataType === 'ENTITY' && column.specificEntityType)
    return {
      kind: 'mentions',
      entityType: column.specificEntityType,
      ids: listedValues(value),
    };
  if (column.dataType === 'BOOLEAN' && !column.isMultiSelect)
    return {
      kind: 'boolean',
      checked: value === 1 || value === '1' || value === 'true',
    };
  if (SELECT_TYPES.includes(column.dataType))
    return {
      kind: 'options',
      labels: column.isMultiSelect ? listedValues(value) : [String(value)],
    };
  if (column.dataType === 'STRING' && !column.isMultiSelect)
    return { kind: 'markdown', markdown: String(value) };
  return { kind: 'text', text: formatCellValue(column, value) };
}

/** A calendar day reads as the grid's date; a moment keeps its time. */
function formatDateValue(value: string): string | undefined {
  const parts =
    /^\d{4}-\d{2}-\d{2}(?:T(\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?)(Z|[+-]\d{2}:?\d{2})?)?$/.exec(
      value
    );
  if (!parts || !Number.isFinite(Date.parse(value))) return;
  const [, time, zone] = parts;
  const midnight = !time || /^00:00(?::00(?:\.0+)?)?$/.test(time);
  const utc = !zone || zone === 'Z' || /^[+-]00:?00$/.test(zone);
  if (midnight && (utc || !time)) {
    const day = fromCellDate(value);
    return day ? formatDate(day) : undefined;
  }
  const moment = new Date(value);
  return `${formatDate(moment)}, ${formatTime(moment)}`;
}

function linkedRecords(value: string | number): ResultCell {
  const count = listedValues(value).length;
  return {
    kind: 'text',
    text: count > 1 ? `${count} linked records` : 'Linked record',
  };
}

/** Multi-valued cells arrive as a JSON array string. */
function listedValues(value: string | number): string[] {
  if (typeof value === 'string' && value.startsWith('[')) {
    try {
      const parsed: unknown = JSON.parse(value);
      if (Array.isArray(parsed)) return parsed.map(String);
    } catch {
      // Not a list after all: the text itself is the value.
    }
  }
  return [String(value)];
}

function resultEntityType(
  type: ResultColumn['entity_type']
): DatabaseEntityType | undefined {
  return match(type)
    .with('user', () => 'USER' as const)
    .with('document', () => 'DOCUMENT' as const)
    .with('channel', () => 'CHANNEL' as const)
    .with('chat', () => 'CHAT' as const)
    .with('project', () => 'PROJECT' as const)
    .with('initiative', () => 'INITIATIVE' as const)
    .with('email_thread', () => 'THREAD' as const)
    .with('crm_company', () => 'COMPANY' as const)
    .with('call', () => 'CALL_RECORD' as const)
    .with('calendar_event', () => 'CALENDAR_EVENT' as const)
    .with('database_row', () => 'DATABASE_ROW' as const)
    .with(P.nullish, P.string, () => undefined)
    .exhaustive();
}

/** A cell as plain text, for chart labels, titles and accessible names. */
export function resultCellText(cell: ResultCell): string {
  return match(cell)
    .with({ kind: 'empty' }, () => '—')
    .with({ kind: 'text' }, ({ text }) => text)
    .with({ kind: 'markdown' }, ({ markdown }) => markdownToPlainText(markdown))
    .with({ kind: 'boolean' }, ({ checked }) => (checked ? 'True' : 'False'))
    .with({ kind: 'options' }, ({ labels }) => labels.join(', '))
    .with({ kind: 'mentions' }, ({ entityType, ids }) =>
      mentionCount(entityType, ids.length)
    )
    .exhaustive();
}

function mentionCount(type: DatabaseEntityType, count: number): string {
  const [one, many] = match(type)
    .with('USER', () => ['person', 'people'])
    .with('DOCUMENT', () => ['document', 'documents'])
    .with('TASK', () => ['task', 'tasks'])
    .with('CHANNEL', () => ['channel', 'channels'])
    .with('PROJECT', 'INITIATIVE', () => ['project', 'projects'])
    .with('CHAT', () => ['chat', 'chats'])
    .with('THREAD', () => ['email', 'emails'])
    .with('COMPANY', () => ['company', 'companies'])
    .with('CALL_RECORD', () => ['call', 'calls'])
    .with('CALENDAR_EVENT', () => ['event', 'events'])
    .with('DATABASE_ROW', () => ['linked record', 'linked records'])
    .exhaustive();
  return `${count} ${count === 1 ? one : many}`;
}
