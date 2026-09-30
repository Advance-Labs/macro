/**
 * SQL text builders for the database grid, in the Macro Databases dialect
 * (`crates/database_sql`).
 *
 * Reads run in the browser engine and writes through `POST /databases/exec`;
 * both take SQL as one string and nothing else — there is no parameter
 * array — so every value is inlined as a literal here. Table and
 * column names arrive from `GET /databases/{id}` already double-quoted
 * (`"Guest List"`) and are used verbatim; nothing in this module should ever
 * concatenate a raw string into a statement.
 */
import type {
  DatabaseColumnDetail,
  SqlValue,
} from '@service-storage/databases';
import { match, P } from 'ts-pattern';
import {
  type DatabaseFilter,
  type DatabaseSort,
  type DatabaseViewColumn,
  type DatabaseViewConfig,
  databaseFilterKind,
  filterNeedsValue,
  filterOperatorsFor,
} from './core/database-view';

/** The virtual row identity column, first in every row-shaped SELECT. */
export const ROW_ID_COLUMN = 'row_id';

/**
 * The virtual column holding a row's place in its table. Rows arrive in no
 * particular order, so every read that lists rows ends its ORDER BY with it.
 */
const ROW_POSITION_COLUMN = 'row_position';

/**
 * The name a result column carries for this table column: its display name,
 * unquoted — `sql_name` is the same name quoted for use in statements.
 */
export function resultColumnName(column: DatabaseColumnDetail): string {
  return (
    column.column.display_name ?? column.definition.definition.display_name
  );
}

/**
 * A value as a write statement spells it: scalars, `TRUE`/`FALSE` for a
 * checkbox, or a list for a multi-valued cell (select labels, entity ids,
 * related row ids).
 */
export type SqlWriteValue =
  | SqlValue
  | boolean
  | undefined
  | readonly (string | number)[];

/**
 * A quoted name from the table detail.
 *
 * An empty name would send a statement naming nothing, so it throws instead
 * of building one.
 */
function sqlName(name: string): string {
  if (!name) throw new Error('SQL name must not be empty');
  return name;
}

/**
 * Render a value as a dialect literal.
 *
 * Strings quote with single quotes, doubling any inside. `NaN` and the
 * infinities have no spelling; writing them as `NULL` would silently clear
 * the cell, so they are rejected rather than translated. A list is written as
 * `['a', 'b']`; the grammar has no empty list, so an empty one clears the cell
 * with `NULL`.
 */
export function sqlLiteral(value: SqlWriteValue): string {
  if (value === null || value === undefined) return 'NULL';
  if (typeof value === 'boolean') return value ? 'TRUE' : 'FALSE';
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) {
      throw new Error(`${value} has no SQL representation`);
    }
    return String(value);
  }
  if (typeof value === 'string') return `'${value.replaceAll("'", "''")}'`;
  if (!value.length) return 'NULL';
  return `[${value.map((item) => sqlLiteral(item)).join(', ')}]`;
}

/** `SELECT * FROM <table>`; `row_id` comes back first. */
export function selectAllStatement(tableSqlName: string): string {
  return `SELECT * FROM ${sqlName(tableSqlName)}`;
}

/** Every row of a table, in the table's order. */
export function tableRowsStatement(tableSqlName: string): string {
  return `${selectAllStatement(tableSqlName)} ORDER BY ${ROW_POSITION_COLUMN}`;
}

/** A view column with the quoted name statements use for it. */
export type ViewStatementColumn = {
  column: DatabaseViewColumn;
  sqlName: string;
};

/** `row_id` is never empty, so these hold for no row and for every row. */
const NO_ROWS = `${ROW_ID_COLUMN} IS NULL`;
const EVERY_ROW = `${ROW_ID_COLUMN} IS NOT NULL`;

/** Conditions that must all hold; a group is parenthesized so it nests. */
function both(conditions: readonly string[]): string | undefined {
  return conditions.length > 1
    ? `(${conditions.join(' AND ')})`
    : conditions[0];
}

/** Conditions of which one must hold; a group is parenthesized so it nests. */
function either(conditions: readonly string[]): string | undefined {
  return conditions.length > 1 ? `(${conditions.join(' OR ')})` : conditions[0];
}

/** The user's text as a LIKE pattern that matches it literally. */
function literally(text: string): string {
  return text.replace(/[\\%_]/g, (character) => `\\${character}`);
}

/** `LIKE` ignores case; `ESCAPE` is spelled only when the pattern uses it. */
function like(
  column: string,
  pattern: string,
  operator: 'LIKE' | 'NOT LIKE' = 'LIKE'
): string {
  const escapeClause = pattern.includes('\\')
    ? ` ESCAPE ${sqlLiteral('\\')}`
    : '';
  return `${column} ${operator} ${sqlLiteral(pattern)}${escapeClause}`;
}

/** The column's option labels whose lower case passes the test. */
function optionLabels(
  column: DatabaseViewColumn,
  test: (label: string) => boolean
): string[] {
  return column.options
    .map(String)
    .filter((label) => test(label.toLocaleLowerCase()));
}

/** Conditions any one of which means the cell holds one of these options. */
function holdingAny(
  { column, sqlName }: ViewStatementColumn,
  labels: readonly string[]
): string[] {
  if (!labels.length) return [];
  if (column.isMultiSelect)
    return labels.map((label) => `${sqlName} HAS ${sqlLiteral(label)}`);
  return [`${sqlName} IN (${labels.map(sqlLiteral).join(', ')})`];
}

/** The cell holds none of these options; a single select must hold another. */
function holdingNone(
  { column, sqlName }: ViewStatementColumn,
  labels: readonly string[]
): string {
  if (column.isMultiSelect)
    return (
      both(labels.map((label) => `${sqlName} NOT HAS ${sqlLiteral(label)}`)) ??
      EVERY_ROW
    );
  return labels.length
    ? `${sqlName} NOT IN (${labels.map(sqlLiteral).join(', ')})`
    : `${sqlName} IS NOT NULL`;
}

/** The cell holds an option whose lower-case label passes the test. */
function holdingOptionWhere(
  target: ViewStatementColumn,
  test: (label: string) => boolean
): string {
  return (
    either(holdingAny(target, optionLabels(target.column, test))) ?? NO_ROWS
  );
}

/** The cell holds no option whose lower-case label passes the test. */
function holdingNoOptionWhere(
  target: ViewStatementColumn,
  test: (label: string) => boolean
): string {
  return holdingNone(target, optionLabels(target.column, test));
}

/** A finite number as a literal. */
function numberLiteral(value: string): string | undefined {
  const number = Number(value);
  return Number.isFinite(number) ? sqlLiteral(number) : undefined;
}

/** A checkbox value, `1` or `0`, as `TRUE` or `FALSE`. */
function checkboxLiteral(value: string): string | undefined {
  const checked = Number(value);
  return checked === 1 || checked === 0 ? sqlLiteral(checked === 1) : undefined;
}

/** A `YYYY-MM-DD` day and the day after it, as literals. */
function calendarDay(value: string): { day: string; next: string } | undefined {
  const text = value.trim();
  const date = new Date(`${text}T00:00:00Z`);
  if (
    !/^\d{4}-\d{2}-\d{2}$/.test(text) ||
    Number.isNaN(date.getTime()) ||
    date.toISOString().slice(0, 10) !== text
  )
    return;
  date.setUTCDate(date.getUTCDate() + 1);
  return {
    day: sqlLiteral(text),
    next: sqlLiteral(date.toISOString().slice(0, 10)),
  };
}

/**
 * One filter as a condition, or nothing when it is ignored: unfinished, or an
 * operator the column does not offer. Cells are compared as the grid shows
 * them: text without case, a multi-select by membership, a date (an instant)
 * by its UTC calendar day.
 */
function filterCondition(
  filter: DatabaseFilter,
  target: ViewStatementColumn
): string | undefined {
  const { column, sqlName: name } = target;
  if (
    !filterOperatorsFor(column).some(({ value }) => value === filter.operator)
  )
    return;
  if (filterNeedsValue(filter.operator) && !filter.value.trim()) return;
  const value = filter.value;
  const lower = value.toLocaleLowerCase();
  const number = numberLiteral(value);
  const checkbox = checkboxLiteral(value);
  const date = calendarDay(value);
  const compared = (operator: string, literal: string | undefined) =>
    literal === undefined ? undefined : `${name} ${operator} ${literal}`;
  return (
    match([filter.operator, databaseFilterKind(column)])
      .with(['is_empty', 'text'], () =>
        either([`${name} IS NULL`, `${name} = ''`])
      )
      .with(['is_empty', P._], () => `${name} IS NULL`)
      .with(['is_not_empty', 'text'], () =>
        both([`${name} IS NOT NULL`, `${name} != ''`])
      )
      .with(['is_not_empty', P._], () => `${name} IS NOT NULL`)
      .with(['equals', 'text'], () => like(name, literally(value)))
      .with(['equals', 'select'], () =>
        holdingOptionWhere(target, (label) => label === lower)
      )
      .with(['equals', 'number'], () => compared('=', number))
      .with(['equals', 'checkbox'], () => compared('=', checkbox))
      .with(
        ['equals', 'date'],
        () => date && both([`${name} >= ${date.day}`, `${name} < ${date.next}`])
      )
      .with(['not_equals', 'text'], () =>
        both([`${name} != ''`, like(name, literally(value), 'NOT LIKE')])
      )
      .with(['not_equals', 'select'], () =>
        holdingNoOptionWhere(target, (label) => label === lower)
      )
      .with(['not_equals', 'number'], () => compared('!=', number))
      .with(['not_equals', 'checkbox'], () => compared('!=', checkbox))
      .with(
        ['not_equals', 'date'],
        () =>
          date && either([`${name} < ${date.day}`, `${name} >= ${date.next}`])
      )
      .with(['contains', 'text'], () => like(name, `%${literally(value)}%`))
      .with(['contains', 'select'], () =>
        holdingOptionWhere(target, (label) => label.includes(lower))
      )
      .with(['not_contains', 'text'], () =>
        both([`${name} != ''`, like(name, `%${literally(value)}%`, 'NOT LIKE')])
      )
      .with(['not_contains', 'select'], () =>
        holdingNoOptionWhere(target, (label) => label.includes(lower))
      )
      .with(['starts_with', 'text'], () => like(name, `${literally(value)}%`))
      .with(['starts_with', 'select'], () =>
        holdingOptionWhere(target, (label) => label.startsWith(lower))
      )
      .with(['gt', 'number'], () => compared('>', number))
      .with(['gte', 'number'], () => compared('>=', number))
      .with(['lt', 'number'], () => compared('<', number))
      .with(['lte', 'number'], () => compared('<=', number))
      .with(['gt', 'date'], () => compared('>=', date?.next))
      .with(['gte', 'date'], () => compared('>=', date?.day))
      .with(['lt', 'date'], () => compared('<', date?.day))
      .with(['lte', 'date'], () => compared('<', date?.next))
      // Not offered for these columns, so never reached.
      .with(
        [
          P.union('gt', 'gte', 'lt', 'lte'),
          P.union('text', 'select', 'checkbox'),
        ],
        [
          P.union('contains', 'not_contains', 'starts_with'),
          P.union('number', 'checkbox', 'date'),
        ],
        [P._, 'reference'],
        () => undefined
      )
      .exhaustive()
  );
}

/**
 * Any text cell containing the term, or any select holding an option whose
 * label does. A term no column can hold matches no row.
 */
function searchCondition(
  search: string,
  columns: readonly ViewStatementColumn[]
): string | undefined {
  const term = search.trim();
  if (!term) return;
  const lower = term.toLocaleLowerCase();
  const conditions = columns.flatMap((target) =>
    match(databaseFilterKind(target.column))
      .with('text', () => [like(target.sqlName, `%${literally(term)}%`)])
      .with('select', () =>
        holdingAny(
          target,
          optionLabels(target.column, (label) => label.includes(lower))
        )
      )
      .with('number', 'checkbox', 'date', 'reference', () => [])
      .exhaustive()
  );
  return either(conditions) ?? NO_ROWS;
}

/**
 * The view's sorts, then the table's order for rows they leave tied.
 * Relations hold row ids, not the labels shown, so they do not sort.
 */
function orderBy(
  sorts: readonly DatabaseSort[],
  columns: ReadonlyMap<string, ViewStatementColumn>
): string {
  const keys = sorts.flatMap((sort) => {
    const target = columns.get(sort.columnId);
    return target && !target.column.relation
      ? [`${target.sqlName} ${sort.direction === 'asc' ? 'ASC' : 'DESC'}`]
      : [];
  });
  return [...keys, ROW_POSITION_COLUMN].join(', ');
}

/**
 * The grid's read for a view: its search, its filters joined by the view's
 * conjunction, and its sorts, all answered by the engine.
 */
export function viewSelectStatement(args: {
  tableSqlName: string;
  columns: readonly ViewStatementColumn[];
  view: DatabaseViewConfig;
}): string {
  const columns = new Map(
    args.columns.map((entry) => [entry.column.id, entry])
  );
  const filters = args.view.filters.flatMap((filter) => {
    const target = columns.get(filter.columnId);
    const condition = target && filterCondition(filter, target);
    return condition ? [condition] : [];
  });
  const where = [
    searchCondition(args.view.search, args.columns),
    args.view.filterConjunction === 'or' ? either(filters) : both(filters),
  ].filter((condition) => condition !== undefined);
  return [
    selectAllStatement(args.tableSqlName),
    ...(where.length ? [`WHERE ${where.join(' AND ')}`] : []),
    `ORDER BY ${orderBy(args.view.sorts, columns)}`,
  ].join(' ');
}

/** Rows by id, whether or not a view would show them. */
export function rowsByIdStatement(
  tableSqlName: string,
  rowIds: readonly string[]
): string {
  if (!rowIds.length) throw new Error('No rows to read');
  return `${selectAllStatement(tableSqlName)} WHERE ${ROW_ID_COLUMN} IN (${rowIds.map((rowId) => sqlLiteral(rowId)).join(', ')})`;
}

/** Update one cell of one row; a list value replaces a multi-valued cell. */
export function updateCellStatement(args: {
  tableSqlName: string;
  columnSqlName: string;
  rowId: string;
  value: SqlWriteValue;
}): string {
  return (
    `UPDATE ${sqlName(args.tableSqlName)} ` +
    `SET ${sqlName(args.columnSqlName)} = ${sqlLiteral(args.value)} ` +
    `WHERE ${ROW_ID_COLUMN} = ${sqlLiteral(args.rowId)}`
  );
}

/**
 * Append an empty row. Row ids are minted by the server, so no `row_id` is
 * supplied; `DEFAULT VALUES` is the only spelling that inserts no columns.
 */
export function insertEmptyRowStatement(tableSqlName: string): string {
  return `INSERT INTO ${sqlName(tableSqlName)} DEFAULT VALUES`;
}

/**
 * Insert a row with named values, including a board card's initial group and
 * any related rows. Keys are the columns' quoted SQL names.
 */
export function insertRowStatement(args: {
  tableSqlName: string;
  values: Record<string, SqlWriteValue>;
}): string {
  const entries = Object.entries(args.values);
  if (!entries.length) return insertEmptyRowStatement(args.tableSqlName);
  if (
    entries.some(
      ([name]) => name === ROW_ID_COLUMN || name === `"${ROW_ID_COLUMN}"`
    )
  )
    throw new Error('Row ids are assigned by the server');
  return (
    `INSERT INTO ${sqlName(args.tableSqlName)} ` +
    `(${entries.map(([name]) => sqlName(name)).join(', ')}) ` +
    `VALUES (${entries.map(([, value]) => sqlLiteral(value)).join(', ')})`
  );
}

/** Remove one row from a table (membership only — entities are untouched). */
export function deleteRowStatement(args: {
  tableSqlName: string;
  rowId: string;
}): string {
  return (
    `DELETE FROM ${sqlName(args.tableSqlName)} ` +
    `WHERE ${ROW_ID_COLUMN} = ${sqlLiteral(args.rowId)}`
  );
}

/** Rows of `tableSqlName` whose relation column links to the given row. */
export function linkedFromStatement(args: {
  tableSqlName: string;
  columnSqlName: string;
  rowId: string;
}): string {
  return (
    `SELECT ${ROW_ID_COLUMN} FROM ${sqlName(args.tableSqlName)} ` +
    `WHERE ${sqlName(args.columnSqlName)} HAS ${sqlLiteral(args.rowId)}`
  );
}
