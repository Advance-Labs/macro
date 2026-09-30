/**
 * Grid edits as the typed ops of `POST /databases/{id}/ops`: a cell edit
 * updates one row, a new record inserts one, a deletion deletes one. Values
 * are the grid's own (labels for options, JSON-array strings for
 * multi-valued cells) and become the op's cell values here.
 */

import type {
  CellValue,
  CellWrite,
  DatabaseOp,
} from '@core/database-sql/generated/types';
import type { DatabaseColumnDetail } from '@service-storage/databases';
import { match } from 'ts-pattern';
import { relatedRowIds } from './database-relations';
import type { DatabaseCellValue } from './database-view';
import type { DatabaseRowMutation } from './table';

const CLEAR: CellValue = { type: 'clear' };

/** Cell values a multi-valued column holds, as the grid's JSON-array string. */
function listedValues(value: DatabaseCellValue): string[] {
  if (typeof value !== 'string' || !value) return [];
  try {
    const parsed: unknown = JSON.parse(value);
    return Array.isArray(parsed)
      ? parsed
          .filter(
            (item): item is string | number =>
              typeof item === 'string' || typeof item === 'number'
          )
          .map(String)
      : [];
  } catch {
    return [];
  }
}

/** The kind of entity an entity column's references point at. */
function referenceKind(column: DatabaseColumnDetail) {
  const target = column.definition.definition.specific_entity_type ?? 'USER';
  if (target === 'DATABASE_ROW')
    throw new Error(
      'This column holds related records; edit it as a relation.'
    );
  return target;
}

/** A calendar day or an instant, as the instant a date cell stores. */
function instant(value: string): string {
  return /^\d{4}-\d{2}-\d{2}$/.test(value) ? `${value}T00:00:00Z` : value;
}

/**
 * A grid value as the value a column's cell takes. An empty value clears
 * any cell but a text one, which keeps the empty text.
 */
export function cellValue(
  column: DatabaseColumnDetail,
  value: DatabaseCellValue
): CellValue {
  const definition = column.definition.definition;
  if (column.column.config?.kind === 'link') {
    const rows = relatedRowIds(value);
    return rows.length ? { type: 'rows', value: rows } : CLEAR;
  }
  if (definition.is_multi_select) {
    const values = listedValues(value);
    if (!values.length) return CLEAR;
    return definition.data_type === 'ENTITY'
      ? {
          type: 'entities',
          value: values.map((entityId) => ({
            entityType: referenceKind(column),
            entityId,
          })),
        }
      : { type: 'options', value: values.map((label) => ({ label })) };
  }
  if (value === null) return CLEAR;
  if (value === '' && definition.data_type !== 'STRING') return CLEAR;
  return match(definition.data_type)
    .returnType<CellValue>()
    .with('STRING', () => ({ type: 'text', value: String(value) }))
    .with('NUMBER', () => {
      const number = typeof value === 'number' ? value : Number(value);
      if (!Number.isFinite(number))
        throw new Error(
          'This column expects a number. Your entry is kept so you can correct it.'
        );
      return { type: 'number', value: number };
    })
    .with('BOOLEAN', () => ({
      type: 'boolean',
      value:
        typeof value === 'number'
          ? value !== 0
          : ['1', 'true'].includes(value.toLowerCase()),
    }))
    .with('DATE', () => ({ type: 'date', value: instant(String(value)) }))
    .with('LINK', () => ({ type: 'link', value: [String(value)] }))
    .with('SELECT_STRING', 'SELECT_NUMBER', 'TAG', () => ({
      type: 'options',
      value: [{ label: String(value) }],
    }))
    .with('ENTITY', () => ({
      type: 'entities',
      value: [{ entityType: referenceKind(column), entityId: String(value) }],
    }))
    .exhaustive();
}

/**
 * The one op a grid edit is. `columnFor` answers the writable column a
 * value goes to; `createOptions` lets labels the column lacks become new
 * options instead of refusing the write.
 */
export function mutationOp(
  tableId: string,
  mutation: DatabaseRowMutation,
  columnFor: (columnId: string) => DatabaseColumnDetail,
  createOptions: boolean
): DatabaseOp {
  const cell = (columnId: string, value: DatabaseCellValue): CellWrite => ({
    column: columnId,
    value: cellValue(columnFor(columnId), value),
  });
  return match(mutation)
    .returnType<DatabaseOp>()
    .with({ kind: 'cell' }, ({ rowId, columnId, value }) => ({
      kind: 'update_rows',
      table: tableId,
      changes: {
        kind: 'per_row',
        rows: [{ row: rowId, cells: [cell(columnId, value)] }],
      },
      createMissingOptions: createOptions,
    }))
    .with({ kind: 'create' }, ({ values }) => ({
      kind: 'insert_rows',
      table: tableId,
      rows: [
        Object.entries(values).map(([columnId, value]) =>
          cell(columnId, value)
        ),
      ],
      createMissingOptions: createOptions,
    }))
    .with({ kind: 'delete' }, ({ rowId }) => ({
      kind: 'delete_rows',
      table: tableId,
      rows: [rowId],
    }))
    .exhaustive();
}
