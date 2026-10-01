/**
 * SQL text for the reads views do not cover, in the Macro Databases dialect
 * (`crates/database_sql`): a table's every row, and rows by id. A view's rows
 * run as the view itself (`runView`); writes are typed ops (`core/cell-ops.ts`).
 *
 * The engine takes SQL as one string and nothing else — there is no
 * parameter array — so every value is inlined as a literal here. Table and
 * column names arrive from `GET /databases/{id}` already double-quoted
 * (`"Guest List"`) and are used verbatim; nothing in this module should ever
 * concatenate a raw string into a statement.
 */

/** The virtual row identity column. */
const ROW_ID_COLUMN = 'row_id';

/**
 * The virtual column holding a row's place in its table. Rows arrive in no
 * particular order, so every read that lists rows ends its ORDER BY with it.
 */
const ROW_POSITION_COLUMN = 'row_position';

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
 * infinities have no spelling, so they are rejected rather than translated.
 */
export function sqlLiteral(value: string | number | boolean): string {
  if (typeof value === 'boolean') return value ? 'TRUE' : 'FALSE';
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) {
      throw new Error(`${value} has no SQL representation`);
    }
    return String(value);
  }
  return `'${value.replaceAll("'", "''")}'`;
}

/** `SELECT * FROM <table>`; `row_id` comes back first. */
export function selectAllStatement(tableSqlName: string): string {
  return `SELECT * FROM ${sqlName(tableSqlName)}`;
}

/** Every row of a table, in the table's order. */
export function tableRowsStatement(tableSqlName: string): string {
  return `${selectAllStatement(tableSqlName)} ORDER BY ${ROW_POSITION_COLUMN}`;
}

/** Rows by id, whether or not a view would show them. */
export function rowsByIdStatement(
  tableSqlName: string,
  rowIds: readonly string[]
): string {
  if (!rowIds.length) throw new Error('No rows to read');
  return `${selectAllStatement(tableSqlName)} WHERE ${ROW_ID_COLUMN} IN (${rowIds.map((rowId) => sqlLiteral(rowId)).join(', ')})`;
}
