import { match } from 'ts-pattern';
import type { DatabaseEntityType } from './column-inference';
import {
  mergeDatabaseColumnOrder,
  reorderDatabaseColumns,
} from './column-order';
import { relatedRowIds } from './database-relations';

export type DatabaseCellValue = string | number | null;

/** View state uses stable column ids, never names or SQL expressions. */
export type DatabaseViewColumn = {
  id: string;
  name: string;
  dataType: string;
  isMultiSelect: boolean;
  options: (string | number)[];
  /** Stored option colours by label; tags fall back to the default tag colour. */
  optionColors?: Record<string, string>;
  writable: boolean;
  specificEntityType?: DatabaseEntityType | null;
  /** A new, empty Text column may adopt the type of its first entry. */
  inferType?: boolean;
  /** A relationship points to rows in a table, independently of the property's scalar type. */
  relation?: {
    databaseId: string;
    tableId: string;
    labels?: Record<string, string>;
  };
};

export type DatabaseFilterOperator =
  | 'contains'
  | 'not_contains'
  | 'equals'
  | 'not_equals'
  | 'starts_with'
  | 'is_empty'
  | 'is_not_empty'
  | 'gt'
  | 'gte'
  | 'lt'
  | 'lte';

export type DatabaseFilter = {
  id: string;
  columnId: string;
  operator: DatabaseFilterOperator;
  value: string;
};

export type DatabaseSort = {
  columnId: string;
  direction: 'asc' | 'desc';
};

/** Whether a row must match every filter or any one of them. */
export type DatabaseFilterConjunction = 'and' | 'or';

export type DatabaseViewConfig = {
  layout: 'table' | 'board';
  groupBy: string | null;
  /** Lane keys in the user’s preferred order; new lanes follow alphabetically. */
  groupOrder?: string[];
  /** Manual row ids per lane; a multi-select row can have a different position in each. */
  cardOrder?: Record<string, string[]>;
  filters: DatabaseFilter[];
  /** Omitted on older views, which match every filter. */
  filterConjunction?: DatabaseFilterConjunction;
  sorts: DatabaseSort[];
  hiddenColumns: string[];
  /** Omitted on older views; unlisted columns follow in schema order. */
  columnOrder?: string[];
  search: string;
};

export type SavedDatabaseViewConfig = {
  kind: 'database-view';
  version: 1;
  databaseId: string;
  tableId: string;
  view: DatabaseViewConfig;
};

export type SavedDatabaseView = {
  id: string;
  name: string;
  view: DatabaseViewConfig;
};

export function defaultDatabaseView(): DatabaseViewConfig {
  return {
    layout: 'table',
    groupBy: null,
    filters: [],
    filterConjunction: 'and',
    sorts: [],
    hiddenColumns: [],
    search: '',
  };
}

/** Stable ids preserve layout through schema changes without dropping fresh columns. */
export function orderDatabaseColumns(
  columns: readonly DatabaseViewColumn[],
  order: readonly string[] = []
): DatabaseViewColumn[] {
  const remaining = new Map(columns.map((column) => [column.id, column]));
  const ordered: DatabaseViewColumn[] = [];
  for (const id of order) {
    const column = remaining.get(id);
    if (!column) continue;
    ordered.push(column);
    remaining.delete(id);
  }
  return [...ordered, ...remaining.values()];
}

/** Move past the next visible column; hidden fields keep their place for later. */
export function moveDatabaseViewColumn(
  view: DatabaseViewConfig,
  columns: readonly DatabaseViewColumn[],
  columnId: string,
  direction: 'left' | 'right'
): DatabaseViewConfig {
  const ordered = orderDatabaseColumns(columns, view.columnOrder);
  const visible = ordered.filter(
    (column) => !view.hiddenColumns.includes(column.id)
  );
  const index = visible.findIndex((column) => column.id === columnId);
  const neighbor = visible[index + (direction === 'left' ? -1 : 1)];
  if (index < 0 || !neighbor) return view;
  const columnOrder = reorderDatabaseColumns(
    ordered.map((column) => column.id),
    view.hiddenColumns,
    columnId,
    neighbor.id,
    direction === 'left' ? 'before' : 'after'
  );
  return columnOrder
    ? reconcileDatabaseView({ ...view, columnOrder }, columns)
    : view;
}

export const FILTER_OPERATORS: {
  value: DatabaseFilterOperator;
  label: string;
}[] = [
  { value: 'contains', label: 'contains' },
  { value: 'not_contains', label: 'does not contain' },
  { value: 'equals', label: 'is' },
  { value: 'not_equals', label: 'is not' },
  { value: 'starts_with', label: 'starts with' },
  { value: 'gt', label: 'is greater than' },
  { value: 'gte', label: 'is at least' },
  { value: 'lt', label: 'is less than' },
  { value: 'lte', label: 'is at most' },
  { value: 'is_empty', label: 'is empty' },
  { value: 'is_not_empty', label: 'is not empty' },
];

export function filterNeedsValue(operator: DatabaseFilterOperator): boolean {
  return operator !== 'is_empty' && operator !== 'is_not_empty';
}

/** How the database engine can compare a column's cells. */
export type DatabaseFilterKind =
  | 'text'
  | 'number'
  | 'checkbox'
  | 'date'
  | 'select'
  | 'reference';

/** Relations and entity references hold ids, so they only filter by emptiness. */
export function databaseFilterKind(
  column: DatabaseViewColumn
): DatabaseFilterKind {
  if (column.relation) return 'reference';
  return match<string, DatabaseFilterKind>(column.dataType)
    .with('STRING', 'LINK', () => 'text')
    .with('NUMBER', () => 'number')
    .with('BOOLEAN', () => 'checkbox')
    .with('DATE', () => 'date')
    .with('SELECT_STRING', 'SELECT_NUMBER', 'TAG', () => 'select')
    .otherwise(() => 'reference');
}

export function filterOperatorsFor(column: DatabaseViewColumn) {
  if (databaseFilterKind(column) === 'reference')
    return FILTER_OPERATORS.filter(
      ({ value }) => value === 'is_empty' || value === 'is_not_empty'
    );
  const numeric = column.dataType === 'NUMBER';
  const date = column.dataType === 'DATE';
  const categorical =
    column.dataType === 'BOOLEAN' || column.dataType.startsWith('SELECT_');
  return FILTER_OPERATORS.filter(({ value }) => {
    if (['gt', 'gte', 'lt', 'lte'].includes(value)) return numeric || date;
    if (['contains', 'not_contains', 'starts_with'].includes(value))
      return !numeric && !date && !categorical;
    return true;
  }).map((operator) => {
    if (column.isMultiSelect && column.dataType.startsWith('SELECT_')) {
      if (operator.value === 'equals')
        return { ...operator, label: 'contains' };
      if (operator.value === 'not_equals')
        return { ...operator, label: 'does not contain' };
    }
    if (!date) return operator;
    const dateLabel: Partial<Record<DatabaseFilterOperator, string>> = {
      gt: 'is after',
      gte: 'is on or after',
      lt: 'is before',
      lte: 'is on or before',
    };
    return { ...operator, label: dateLabel[operator.value] ?? operator.label };
  });
}

export function isBoardGroupColumn(column: DatabaseViewColumn): boolean {
  return (
    !column.relation &&
    ['SELECT_STRING', 'SELECT_NUMBER', 'BOOLEAN'].includes(column.dataType)
  );
}

export function databaseCellValues(
  value: DatabaseCellValue,
  column: DatabaseViewColumn
): DatabaseCellValue[] {
  if (column.relation)
    return relatedRowIds(value).map(
      (id) => column.relation?.labels?.[id] ?? 'Unavailable record'
    );
  if (column.isMultiSelect && typeof value === 'string') {
    try {
      const parsed: unknown = JSON.parse(value);
      if (Array.isArray(parsed))
        return parsed.filter(
          (item): item is DatabaseCellValue =>
            item === null ||
            typeof item === 'string' ||
            typeof item === 'number'
        );
    } catch {
      // Older scalar values remain searchable when a property becomes multi-value.
    }
  }
  return [value];
}

function isEmpty(
  value: DatabaseCellValue,
  column: DatabaseViewColumn
): boolean {
  return databaseCellValues(value, column).every(
    (item) => item === null || item === ''
  );
}

export type DatabaseRowGroup<Row> = {
  key: string;
  label: string;
  value: DatabaseCellValue;
  rows: Row[];
};

/** Configured options stay visible even when empty, so they are drop targets. */
export function groupDatabaseRows<Row>(
  rows: readonly Row[],
  column: DatabaseViewColumn,
  getValue: (row: Row, columnId: string) => DatabaseCellValue
): DatabaseRowGroup<Row>[] {
  const groups = new Map<string, DatabaseRowGroup<Row>>();
  const normalize = (value: DatabaseCellValue): DatabaseCellValue => {
    if (isEmpty(value, column)) return null;
    if (column.dataType === 'BOOLEAN') return Number(value) ? 1 : 0;
    // SELECT_NUMBER, like SELECT_STRING, exposes labels as SQLite TEXT.
    return String(value);
  };
  const addGroup = (rawValue: DatabaseCellValue) => {
    const value = normalize(rawValue);
    const key = value === null ? 'empty' : `value:${JSON.stringify(value)}`;
    let group = groups.get(key);
    if (!group) {
      const label =
        value === null
          ? `No ${column.name.toLocaleLowerCase()}`
          : column.dataType === 'BOOLEAN'
            ? value === 1
              ? 'Checked'
              : 'Unchecked'
            : String(value);
      group = { key, label, value, rows: [] };
      groups.set(key, group);
    }
    return group;
  };
  for (const value of column.dataType === 'BOOLEAN' ? [0, 1] : column.options)
    addGroup(value);
  for (const row of rows) {
    const values = databaseCellValues(getValue(row, column.id), column);
    for (const value of new Set(values.length ? values : [null]))
      addGroup(value).rows.push(row);
  }
  addGroup(null);
  return [...groups.values()].sort((a, b) =>
    a.value === null
      ? 1
      : b.value === null
        ? -1
        : a.label.localeCompare(b.label, undefined, {
            numeric: true,
            sensitivity: 'base',
          })
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isFilter(value: unknown): value is DatabaseFilter {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    typeof value.columnId === 'string' &&
    typeof value.value === 'string' &&
    FILTER_OPERATORS.some((operator) => operator.value === value.operator)
  );
}

function isSort(value: unknown): value is DatabaseSort {
  return (
    isRecord(value) &&
    typeof value.columnId === 'string' &&
    (value.direction === 'asc' || value.direction === 'desc')
  );
}

export function isDatabaseViewConfig(
  value: unknown
): value is DatabaseViewConfig {
  return (
    isRecord(value) &&
    (value.layout === 'table' || value.layout === 'board') &&
    (value.groupBy === null || typeof value.groupBy === 'string') &&
    typeof value.search === 'string' &&
    Array.isArray(value.filters) &&
    value.filters.every(isFilter) &&
    (value.filterConjunction === undefined ||
      value.filterConjunction === 'and' ||
      value.filterConjunction === 'or') &&
    Array.isArray(value.sorts) &&
    value.sorts.every(isSort) &&
    Array.isArray(value.hiddenColumns) &&
    value.hiddenColumns.every((id) => typeof id === 'string') &&
    (value.groupOrder === undefined ||
      (Array.isArray(value.groupOrder) &&
        value.groupOrder.every((id) => typeof id === 'string'))) &&
    (value.cardOrder === undefined ||
      (isRecord(value.cardOrder) &&
        Object.values(value.cardOrder).every(
          (ids) =>
            Array.isArray(ids) && ids.every((id) => typeof id === 'string')
        ))) &&
    (value.columnOrder === undefined ||
      (Array.isArray(value.columnOrder) &&
        value.columnOrder.every((id) => typeof id === 'string')))
  );
}

export function isSavedDatabaseViewConfig(
  value: unknown
): value is SavedDatabaseViewConfig {
  return (
    isRecord(value) &&
    value.kind === 'database-view' &&
    value.version === 1 &&
    typeof value.databaseId === 'string' &&
    typeof value.tableId === 'string' &&
    isDatabaseViewConfig(value.view)
  );
}

/** Adding a field or loading an older view never makes fresh fields disappear. */
export function reconcileDatabaseView(
  view: DatabaseViewConfig,
  columns: readonly DatabaseViewColumn[]
): DatabaseViewConfig {
  const ids = new Set(columns.map((column) => column.id));
  const columnOrder = orderDatabaseColumns(columns, view.columnOrder).map(
    (column) => column.id
  );
  const groupBy = columns.some(
    (column) => column.id === view.groupBy && isBoardGroupColumn(column)
  )
    ? view.groupBy
    : view.layout === 'board'
      ? (columns.find(isBoardGroupColumn)?.id ?? null)
      : null;
  return {
    ...view,
    // Canonical schema order keeps a restored default view from looking unsaved.
    columnOrder: columnOrder.every((id, index) => id === columns[index].id)
      ? undefined
      : columnOrder,
    groupBy,
    cardOrder: groupBy === view.groupBy ? view.cardOrder : undefined,
    filters: view.filters.filter((filter) => ids.has(filter.columnId)),
    filterConjunction: view.filterConjunction ?? 'and',
    sorts: view.sorts.filter((sort) => ids.has(sort.columnId)),
    hiddenColumns: view.hiddenColumns.filter((id) => ids.has(id)),
  };
}

/** Preserve unrelated tags when moving one occurrence of a multi-select card. */
export function boardMoveValue(
  column: DatabaseViewColumn,
  current: DatabaseCellValue,
  target: DatabaseCellValue,
  from?: DatabaseCellValue
): DatabaseCellValue {
  if (!column.isMultiSelect) return target;
  if (target === null) return '[]';
  const values = databaseCellValues(current, column).filter(
    (value) => value !== null && value !== '' && value !== from
  );
  return JSON.stringify([...new Set([...values, target])]);
}

export function orderDatabaseGroups<Row>(
  groups: readonly DatabaseRowGroup<Row>[],
  order: readonly string[] = []
): DatabaseRowGroup<Row>[] {
  const remaining = new Map(groups.map((group) => [group.key, group]));
  const ordered: DatabaseRowGroup<Row>[] = [];
  for (const key of order) {
    const group = remaining.get(key);
    if (group) {
      ordered.push(group);
      remaining.delete(key);
    }
  }
  return [...ordered, ...remaining.values()];
}

/** Ignore stale ids and append newly visible cards in their existing row order. */
export function orderDatabaseCards<Row>(
  rows: readonly Row[],
  order: readonly string[] | undefined,
  getId: (row: Row) => string
): Row[] {
  const remaining = new Map(rows.map((row) => [getId(row), row]));
  const ordered: Row[] = [];
  for (const id of order ?? []) {
    const row = remaining.get(id);
    if (row === undefined) continue;
    ordered.push(row);
    remaining.delete(id);
  }
  return [...ordered, ...remaining.values()];
}

/** Insert at the displayed gap without moving filtered-out cards from their slots. */
export function placeDatabaseCard(
  order: readonly string[],
  visibleOrder: readonly string[],
  rowId: string,
  beforeId?: string
): string[] {
  if (
    beforeId === rowId ||
    (beforeId !== undefined && !visibleOrder.includes(beforeId))
  )
    return [...order];
  const visible = [...new Set(visibleOrder)].filter((id) => id !== rowId);
  const insertion =
    beforeId === undefined ? visible.length : visible.indexOf(beforeId);
  visible.splice(insertion, 0, rowId);
  const completeOrder = [...new Set([...order, ...visibleOrder, rowId])];
  return mergeDatabaseColumnOrder(completeOrder, visible);
}
