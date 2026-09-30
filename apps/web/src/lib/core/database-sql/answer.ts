/**
 * The engine's outcome as the exec API answers a read, so every renderer
 * that decoded the server's result sets decodes the browser's the same way.
 * Mirrors `run_sql` and `result_sets` in
 * `crates/databases/src/domain/service/query.rs`.
 */

import type {
  DatabaseColumnDetail,
  DatabaseDetail,
  ExecOutcome,
  QueryResult,
  ResultColumn,
  SqlValue,
} from '@service-storage/databases';
import { match } from 'ts-pattern';
import type { Catalog, CatalogTable, Cell, Outcome } from './protocol';

/** The read half of an exec outcome. */
export type DatabaseSqlAnswer = Pick<
  ExecOutcome,
  'results' | 'read_tables' | 'read_versions' | 'truncated_tables'
> & { read_database_ids: string[] };

/** The column a definition is bound to, first in catalog order. */
type BoundColumn = {
  table: CatalogTable;
  column: CatalogTable['columns'][number];
  detail: DatabaseColumnDetail | undefined;
};

export function databaseSqlAnswer(
  outcome: Outcome,
  catalog: Catalog,
  databases: readonly DatabaseDetail[]
): DatabaseSqlAnswer {
  const tables = new Map(
    databases.flatMap((database) =>
      database.tables.map((table) => [table.table.id, table] as const)
    )
  );
  const boundColumn = (definition: string | undefined) => {
    if (!definition) return undefined;
    for (const table of catalog.tables) {
      const column = table.columns.find(
        (candidate) => candidate.id === definition
      );
      if (column)
        return {
          table,
          column,
          detail: tables
            .get(table.id)
            ?.columns.find(
              (candidate) => candidate.definition.definition.id === definition
            ),
        } satisfies BoundColumn;
    }
  };
  const read = outcome.readTables.flatMap((id) => {
    const table = tables.get(id);
    return table ? [table] : [];
  });
  return {
    results: resultSets(outcome, boundColumn),
    read_tables: outcome.readTables,
    read_database_ids: [
      ...new Set(read.map((table) => table.table.database_id)),
    ],
    read_versions: Object.fromEntries(
      read.map((table) => [table.table.id, table.table.version])
    ),
    truncated_tables: outcome.truncated
      ? read.map((table) => table.table.name)
      : [],
  };
}

function resultSets(
  outcome: Outcome,
  boundColumn: (definition: string | undefined) => BoundColumn | undefined
): QueryResult[] {
  if (outcome.columns.length === 0) return [];
  const rowShaped =
    (outcome.rowIds.length === outcome.rows.length &&
      outcome.rows.length > 0) ||
    (outcome.rows.length === 0 &&
      outcome.columns.every((column) => column.column !== undefined));
  const bound = outcome.columns.map((column) => boundColumn(column.column));
  const columns: ResultColumn[] = outcome.columns.map((column, index) => ({
    name: column.name,
    entity_type:
      column.kind === 'entity' ? entityTypeOf(bound[index]?.detail) : null,
    origin: bound[index]
      ? [bound[index].table.name, bound[index].column.name]
      : null,
  }));
  return [
    {
      columns: rowShaped
        ? [{ name: 'row_id', entity_type: null, origin: null }, ...columns]
        : columns,
      rows: outcome.rows.map((row, index) => {
        const values = row.map((cell, position) =>
          sqlValue(cell, bound[position])
        );
        return rowShaped ? [outcome.rowIds[index], ...values] : values;
      }),
    },
  ];
}

/**
 * A cell as the scalar the clients decode: labels for options, ids for
 * references, a checkbox as 0 or 1, a date as RFC 3339, and a JSON array
 * for a multi-valued cell.
 */
function sqlValue(cell: Cell | null, bound: BoundColumn | undefined): SqlValue {
  if (!cell) return null;
  const kind = bound?.column.kind;
  const multi =
    (kind?.kind === 'select' || kind?.kind === 'entity') && kind.multi;
  return (
    match(cell)
      .returnType<SqlValue>()
      .with({ type: 'text' }, ({ value }) => value)
      .with({ type: 'number' }, ({ value }) => value)
      .with({ type: 'bool' }, ({ value }) => (value ? 1 : 0))
      // The engine writes UTC as `Z`; the server spells it `+00:00`.
      .with({ type: 'date' }, ({ value }) => value.replace(/Z$/, '+00:00'))
      .with({ type: 'options' }, ({ value }) =>
        scalarOrArray(
          value.map(
            (id) =>
              (kind?.kind === 'select'
                ? kind.options.find((option) => option.id === id)?.label
                : undefined) ?? id
          ),
          multi
        )
      )
      .with({ type: 'entities' }, ({ value }) => scalarOrArray(value, multi))
      .exhaustive()
  );
}

function scalarOrArray(values: string[], multi: boolean): SqlValue {
  return multi ? JSON.stringify(values) : (values[0] ?? null);
}

/** The platform entity type an entity column's references point at. */
function entityTypeOf(
  column: DatabaseColumnDetail | undefined
): ResultColumn['entity_type'] {
  if (!column) return null;
  if (column.column.config?.kind === 'link') return 'database_row';
  const target = column.definition.definition.specific_entity_type;
  if (!target) return null;
  return match(target)
    .returnType<string>()
    .with('USER', () => 'user')
    .with('DOCUMENT', 'TASK', () => 'document')
    .with('COMPANY', () => 'crm_company')
    .with('CALL_RECORD', () => 'call')
    .with('CHANNEL', () => 'channel')
    .with('CHAT', () => 'chat')
    .with('PROJECT', () => 'project')
    .with('THREAD', () => 'email_thread')
    .with('CALENDAR_EVENT', () => 'calendar_event')
    .with('INITIATIVE', () => 'initiative')
    .with('DATABASE_ROW', () => 'database_row')
    .exhaustive();
}
