/**
 * SQL text builders for the database grid, in the Macro Databases dialect
 * (`crates/database_sql`).
 *
 * `POST /databases/exec` takes SQL as one string and nothing else — there is
 * no parameter array — so every value is inlined as a literal here. Table and
 * column names arrive from `GET /databases/{id}` already double-quoted
 * (`"Guest List"`) and are used verbatim; nothing in this module should ever
 * concatenate a raw string into a statement.
 */
import type {
  DatabaseColumnDetail,
  SqlValue,
} from '@service-storage/databases';

/** The virtual row identity column, first in every row-shaped SELECT. */
export const ROW_ID_COLUMN = 'row_id';

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

/** `SELECT * FROM <table>` — the grid's read; `row_id` comes back first. */
export function selectAllStatement(tableSqlName: string): string {
  return `SELECT * FROM ${sqlName(tableSqlName)}`;
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

/**
 * One page of an export. Rows come back in table position order, so paging
 * is stable as long as the table version does not change between pages —
 * which the caller checks.
 */
export function exportPageStatement(
  tableSqlName: string,
  offset: number,
  limit: number
): string {
  if (
    !Number.isSafeInteger(offset) ||
    offset < 0 ||
    !Number.isSafeInteger(limit) ||
    limit < 1
  )
    throw new Error('Invalid export page');
  return `${selectAllStatement(tableSqlName)} LIMIT ${limit} OFFSET ${offset}`;
}
