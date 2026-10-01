/**
 * The engine's cells as the grid's editing values: option labels, ids, a
 * checkbox as 0 or 1, and a JSON array for a multi-valued cell.
 */

import type {
  Catalog,
  Cell,
  ColumnKind,
  Outcome,
} from '@core/database-sql/generated/types';
import type { ColumnDetail } from '@service-storage/generated/schemas/columnDetail';
import { match } from 'ts-pattern';
import type { DatabaseCellValue } from './database-view';
import type { DatabaseRow } from './table';

function listed(values: string[], multi: boolean): DatabaseCellValue {
  return multi ? JSON.stringify(values) : (values[0] ?? null);
}

function gridValue(
  cell: Cell | null,
  kind: ColumnKind | undefined
): DatabaseCellValue {
  if (!cell) return null;
  const multi =
    (kind?.kind === 'select' || kind?.kind === 'entity') && kind.multi;
  return match(cell)
    .returnType<DatabaseCellValue>()
    .with({ type: 'text' }, ({ value }) => value)
    .with({ type: 'number' }, ({ value }) => value)
    .with({ type: 'bool' }, ({ value }) => (value ? 1 : 0))
    .with({ type: 'date' }, ({ value }) => value)
    .with({ type: 'options' }, ({ value }) =>
      listed(
        value.flatMap((id) =>
          kind?.kind === 'select'
            ? kind.options
                .filter((option) => option.id === id)
                .map((option) => option.label)
            : []
        ),
        multi
      )
    )
    .with({ type: 'entities' }, ({ value }) => listed(value, multi))
    .exhaustive();
}

/** A row-shaped outcome's rows, with a cell for each of the table's columns. */
export function gridRows(
  outcome: Outcome,
  catalog: Catalog,
  columns: readonly ColumnDetail[]
): DatabaseRow[] {
  const catalogColumns = new Map(
    catalog.tables.flatMap((table) =>
      table.columns.map((column) => [column.id, column] as const)
    )
  );
  const placed = columns.map((column) => {
    const definition = column.definition.definition.id;
    return {
      id: column.column.id,
      index: outcome.columns.findIndex(
        (candidate) => candidate.column === definition
      ),
      kind: catalogColumns.get(definition)?.kind,
    };
  });
  return outcome.rowIds.map((rowId, rowIndex) => ({
    rowId,
    cells: Object.fromEntries(
      placed.map(({ id, index, kind }) => [
        id,
        gridValue(outcome.rows[rowIndex]?.[index] ?? null, kind),
      ])
    ),
  }));
}
