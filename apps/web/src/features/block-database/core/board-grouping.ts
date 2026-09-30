import { type DatabaseViewColumn, isBoardGroupColumn } from './database-view';

/** A multi-select partitions a board best, then a single select, then a checkbox. */
export function defaultBoardGroupColumn(
  columns: readonly DatabaseViewColumn[]
): DatabaseViewColumn | undefined {
  const groupable = columns.filter(isBoardGroupColumn);
  const isSelect = (column: DatabaseViewColumn) =>
    column.dataType.startsWith('SELECT_');
  return (
    groupable.find((column) => isSelect(column) && column.isMultiSelect) ??
    groupable.find((column) => isSelect(column) && !column.isMultiSelect) ??
    groupable[0]
  );
}
