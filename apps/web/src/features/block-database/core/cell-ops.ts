/**
 * Grid edits as the typed ops of `POST /databases/{id}/ops`, from the grid's
 * own values (option labels, JSON-array strings for multi-valued cells).
 */

import type {
  CellValue,
  CellWrite,
  DatabaseOp,
  OpEntityKind,
} from '@core/database-sql/generated/types';
import type { ColumnDetail } from '@service-storage/generated/schemas/columnDetail';
import { err, ok, Result } from 'neverthrow';
import { match } from 'ts-pattern';
import { inferDatabaseNumber } from './column-inference';
import type { DatabaseCellValue } from './database-view';
import { UNAVAILABLE_OPTION } from './grid-cells';
import type { DatabaseRowMutation } from './table';
import type { DatabaseCellFailure } from './write-failure';

const CLEAR: CellValue = { type: 'clear' };

/**
 * Cell values a multi-valued column holds, from the grid's JSON-array string.
 * Anything else is refused rather than read as no values, which would clear
 * the cell.
 */
function listedValues(
  value: DatabaseCellValue
): Result<string[], DatabaseCellFailure> {
  if (value === null || value === '') return ok([]);
  if (typeof value !== 'string') return err({ kind: 'malformed-list' });
  const parsed = Result.fromThrowable(
    (text: string): unknown => JSON.parse(text),
    (): DatabaseCellFailure => ({ kind: 'malformed-list' })
  )(value);
  return parsed.andThen(
    (list): Result<string[], DatabaseCellFailure> =>
      Array.isArray(list) &&
      list.every((item) => typeof item === 'string' || typeof item === 'number')
        ? ok(list.map(String))
        : err({ kind: 'malformed-list' })
  );
}

/** The kind of entity an entity column's references point at. */
function referenceKind(
  column: ColumnDetail
): Result<OpEntityKind, DatabaseCellFailure> {
  const target = column.definition.definition.specific_entity_type;
  if (target === null) return err({ kind: 'untyped-entity' });
  if (target === 'DATABASE_ROW') return err({ kind: 'relation-as-entity' });
  return ok(target);
}

/** Options named by label; one the grid could not name is refused. */
function optionLabels(
  labels: string[]
): Result<CellValue, DatabaseCellFailure> {
  return labels.includes(UNAVAILABLE_OPTION)
    ? err({ kind: 'unavailable-option' })
    : ok({ type: 'options', value: labels.map((label) => ({ label })) });
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
  column: ColumnDetail,
  value: DatabaseCellValue
): Result<CellValue, DatabaseCellFailure> {
  const definition = column.definition.definition;
  if (column.column.config?.kind === 'link')
    return listedValues(value).map(
      (rows): CellValue =>
        rows.length ? { type: 'rows', value: [...new Set(rows)] } : CLEAR
    );
  if (definition.is_multi_select)
    return listedValues(value).andThen(
      (values): Result<CellValue, DatabaseCellFailure> => {
        if (!values.length) return ok(CLEAR);
        return definition.data_type === 'ENTITY'
          ? referenceKind(column).map((entityType) => ({
              type: 'entities',
              value: values.map((entityId) => ({ entityType, entityId })),
            }))
          : optionLabels(values);
      }
    );
  if (value === null) return ok(CLEAR);
  if (value === '' && definition.data_type !== 'STRING') return ok(CLEAR);
  return match(definition.data_type)
    .returnType<Result<CellValue, DatabaseCellFailure>>()
    .with('STRING', () => ok({ type: 'text', value: String(value) }))
    .with('NUMBER', () => {
      const number =
        typeof value === 'number' ? value : inferDatabaseNumber(value);
      return number !== undefined && Number.isFinite(number)
        ? ok({ type: 'number', value: number })
        : err({ kind: 'not-a-number' });
    })
    .with('BOOLEAN', () =>
      ok({
        type: 'boolean',
        value:
          typeof value === 'number'
            ? value !== 0
            : ['1', 'true'].includes(value.toLowerCase()),
      })
    )
    .with('DATE', () => ok({ type: 'date', value: instant(String(value)) }))
    .with('LINK', () => ok({ type: 'link', value: [String(value)] }))
    .with('SELECT_STRING', 'SELECT_NUMBER', 'TAG', () =>
      optionLabels([String(value)])
    )
    .with('ENTITY', () =>
      referenceKind(column).map((entityType) => ({
        type: 'entities',
        value: [{ entityType, entityId: String(value) }],
      }))
    )
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
  columnFor: (columnId: string) => Result<ColumnDetail, DatabaseCellFailure>,
  createOptions: boolean
): Result<DatabaseOp, DatabaseCellFailure> {
  const cell = (columnId: string, value: DatabaseCellValue) =>
    columnFor(columnId)
      .andThen((column) => cellValue(column, value))
      .map((written): CellWrite => ({ column: columnId, value: written }));
  return match(mutation)
    .returnType<Result<DatabaseOp, DatabaseCellFailure>>()
    .with({ kind: 'cell' }, ({ rowId, columnId, value }) =>
      cell(columnId, value).map((written) => ({
        kind: 'update_rows',
        table: tableId,
        changes: {
          kind: 'per_row',
          rows: [{ row: rowId, cells: [written] }],
        },
        createMissingOptions: createOptions,
      }))
    )
    .with({ kind: 'create' }, ({ values }) =>
      Result.combine(
        Object.entries(values).map(([columnId, value]) => cell(columnId, value))
      ).map((cells) => ({
        kind: 'insert_rows',
        table: tableId,
        rows: [cells],
        createMissingOptions: createOptions,
      }))
    )
    .with({ kind: 'delete' }, ({ rowId }) =>
      ok({ kind: 'delete_rows', table: tableId, rows: [rowId] })
    )
    .exhaustive();
}
