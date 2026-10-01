import type { DatabaseViewColumn } from '../core/database-view';

/** A select or tag column: its cells hold labels from the column's options. */
export function isOptionColumn(
  column: Pick<DatabaseViewColumn, 'dataType' | 'relation'>
): boolean {
  return (
    !column.relation &&
    (column.dataType === 'SELECT_STRING' ||
      column.dataType === 'SELECT_NUMBER' ||
      column.dataType === 'TAG')
  );
}
