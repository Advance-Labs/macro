import { databaseSqlAnswer } from '@core/database-sql/answer';
import { databaseSqlCatalog } from '@core/database-sql/catalog';
import { DatabaseSqlError } from '@core/database-sql/driver';
import { throwOnErr } from '@core/util/result';
import {
  createDatabaseSqlQuery,
  type DatabaseSqlQuery,
  type DatabaseSqlQueryCapabilities,
  type DatabaseSqlStatement,
  refreshInBackground,
  sameDatabaseSqlStatement,
} from '@queries/database-sql/create-database-sql-query';
import { databasesKeys } from '@queries/storage/keys';
import { storageServiceClient } from '@service-storage/client';
import type {
  DatabaseColumnDetail,
  DatabaseDetail,
  DatabaseTableDetail,
  ExecOutcome,
  ExecRequest,
} from '@service-storage/databases';
import { useQueryClient } from '@tanstack/solid-query';
import { type Accessor, createMemo, createSignal, untrack } from 'solid-js';
import {
  type DatabaseRowsSource,
  DatabaseWriteOutcomeUnknown,
} from '../context/table-source';
import {
  type DatabaseColumnType,
  inferDatabaseNumber,
} from '../core/column-inference';
import { relatedRowIds } from '../core/database-relations';
import type {
  DatabaseCellValue,
  DatabaseViewColumn,
  DatabaseViewConfig,
} from '../core/database-view';
import type { DatabaseRow, DatabaseRowMutation } from '../core/table';
import {
  deleteRowStatement,
  insertRowStatement,
  ROW_ID_COLUMN,
  resultColumnName,
  rowsByIdStatement,
  type SqlWriteValue,
  updateCellStatement,
  viewSelectStatement,
} from '../sql';

export function toViewColumn(column: DatabaseColumnDetail): DatabaseViewColumn {
  const relation =
    column.column.config?.kind === 'link' ? column.column.config : undefined;
  return {
    id: column.column.id,
    name:
      column.column.display_name ?? column.definition.definition.display_name,
    dataType: column.definition.definition.data_type,
    isMultiSelect: !!relation || column.definition.definition.is_multi_select,
    options: column.definition.property_options.map((option) =>
      String(option.value.value)
    ),
    optionColors: Object.fromEntries(
      column.definition.property_options.flatMap((option) =>
        option.color ? [[String(option.value.value), option.color]] : []
      )
    ),
    writable: column.writable,
    ...(relation
      ? {
          relation: {
            databaseId: relation.database_id,
            tableId: relation.table_id,
          },
        }
      : {}),
    specificEntityType: column.definition.definition.specific_entity_type,
    inferType: column.column.infer_type ?? false,
  };
}

/** Cell values a multi-valued column holds, as the grid's JSON-array string. */
function listedValues(value: DatabaseCellValue): (string | number)[] {
  if (typeof value !== 'string' || !value) return [];
  try {
    const parsed: unknown = JSON.parse(value);
    return Array.isArray(parsed)
      ? parsed.filter(
          (item): item is string | number =>
            typeof item === 'string' || typeof item === 'number'
        )
      : [];
  } catch {
    return [];
  }
}

/**
 * Spell a grid cell value the way the column's write expects it: a list of
 * row ids for a relation, a list of labels for a multi select, a list of ids
 * for a multi entity, `TRUE`/`FALSE` for a checkbox, and labels as text for a
 * single select. An empty string clears anything but a text cell.
 */
export function writeValue(
  column: DatabaseColumnDetail,
  value: DatabaseCellValue
): SqlWriteValue {
  const definition = column.definition.definition;
  if (column.column.config?.kind === 'link') return relatedRowIds(value);
  if (definition.is_multi_select) return listedValues(value).map(String);
  if (value === null) return null;
  if (definition.data_type === 'BOOLEAN')
    return typeof value === 'number'
      ? value !== 0
      : ['1', 'true'].includes(value.toLowerCase());
  if (value === '' && !['STRING', 'LINK'].includes(definition.data_type))
    return null;
  if (definition.data_type.startsWith('SELECT_')) return String(value);
  return value;
}

/** Lookup columns are not part of the grid, so a view never names them. */
function isGridColumn(column: DatabaseColumnDetail) {
  return column.column.config?.kind !== 'lookup';
}

/** A stale table or column name, which a refreshed schema may resolve. */
function isSqlError(error: unknown): error is Error {
  return (
    error instanceof DatabaseSqlError ||
    (error instanceof Error && 'code' in error && error.code === 'SQL_ERROR')
  );
}

function readError(error: unknown): Error | undefined {
  if (error === undefined || error instanceof Error) return error;
  return new Error(String(error));
}

function sameIds(left: readonly string[], right: readonly string[]) {
  return (
    left.length === right.length &&
    left.every((id, index) => id === right[index])
  );
}

export function createDatabaseRowsSource(props: {
  databaseId: string;
  table: Accessor<DatabaseTableDetail>;
  /** The engine searches, filters and sorts the rows for this view. */
  view: Accessor<DatabaseViewConfig>;
  /** Writes; reads run in the browser's SQL engine. */
  exec: (request: ExecRequest) => Promise<ExecOutcome>;
  /** Where the engine reads rows from; the app's GraphQL client by default. */
  read?: DatabaseSqlQueryCapabilities;
  /** Calls back with the version of each change the gateway reports for this table. */
  onTableChanged: (listener: (version: number) => void) => void;
  applyVersions: (versions: Record<string, number>) => void;
  addOption: (columnId: string, label: string) => Promise<void>;
}): DatabaseRowsSource {
  const queryClient = useQueryClient();
  const tableId = props.table().table.id;
  const detailKey = databasesKeys.detail(props.databaseId).queryKey;
  let staleSchemaError: Error | undefined;
  // Only advance across schema changes this writer has itself acknowledged.
  const inferredVersions = new Map<number, number>();
  const cachedDetail = () =>
    queryClient.getQueryData<DatabaseDetail>(detailKey);
  const currentTable = () =>
    cachedDetail()?.tables.find((entry) => entry.table.id === tableId) ??
    props.table();
  // Reads are rebuilt from the cached schema: a refreshed table name must
  // reach the retry even when the table prop has not caught up.
  const [schemaRefreshes, setSchemaRefreshes] = createSignal(0);

  async function refreshSchema() {
    await queryClient.cancelQueries({ queryKey: detailKey, exact: true });
    const detail = await queryClient.fetchQuery({
      queryKey: detailKey,
      queryFn: () =>
        throwOnErr(() =>
          storageServiceClient.databases.get({ id: props.databaseId })
        ),
      staleTime: 0,
      retry: false,
    });
    if (!detail.tables.some((entry) => entry.table.id === tableId))
      throw new Error('This table is no longer available.');
    staleSchemaError = undefined;
    setSchemaRefreshes((count) => count + 1);
  }
  // Version-only schema updates must not recreate columns and remount editors.
  const details = createMemo(() => props.table().columns);
  const columns = createMemo(() =>
    details().filter(isGridColumn).map(toViewColumn)
  );
  /** A statement over this table alone, built from the cached schema. */
  const tableStatement = (
    sql: (table: DatabaseTableDetail) => string | undefined
  ) =>
    createMemo(
      (): DatabaseSqlStatement | undefined => {
        props.table();
        schemaRefreshes();
        const detail = cachedDetail();
        if (!detail) throw new Error('The database is not loaded.');
        const table = currentTable();
        const text = sql(table);
        if (text === undefined) return undefined;
        return {
          catalog: databaseSqlCatalog(
            [{ ...detail, tables: [table] }],
            props.databaseId
          ),
          sql: text,
        };
      },
      undefined,
      { equals: sameDatabaseSqlStatement }
    );
  const viewStatement = tableStatement((table) =>
    viewSelectStatement({
      tableSqlName: table.sql_name,
      columns: table.columns.filter(isGridColumn).map((column) => ({
        column: toViewColumn(column),
        sqlName: column.sql_name,
      })),
      view: props.view(),
    })
  );
  const rowsQuery = createDatabaseSqlQuery(viewStatement, props.read);
  const [retainedIds, setRetainedIds] = createSignal<
    Accessor<readonly string[]>
  >(() => []);
  const retainedRowIds = createMemo(
    () => [...new Set(retainedIds()())].sort(),
    [],
    { equals: sameIds }
  );
  const retainedStatement = tableStatement((table) =>
    retainedRowIds().length
      ? rowsByIdStatement(table.sql_name, retainedRowIds())
      : undefined
  );
  const retainedQuery = createDatabaseSqlQuery(retainedStatement, props.read);
  // The table's version when the last completed read began. Accepted draft
  // writes can outlive this owner, so it advances on any awaited refresh.
  const [readVersion, setReadVersion] = createSignal(
    untrack(() => currentTable().table.version)
  );

  function rowsOf(
    query: DatabaseSqlQuery,
    statement: Accessor<DatabaseSqlStatement | undefined>
  ): DatabaseRow[] | undefined {
    const outcome = query.outcome();
    const current = statement();
    if (!outcome || !current) return undefined;
    const result = databaseSqlAnswer(outcome, current.catalog, []).results[0];
    const rowIdIndex =
      result?.columns.findIndex((column) => column.name === ROW_ID_COLUMN) ??
      -1;
    const indexes = props.table().columns.map((column) => ({
      id: column.column.id,
      index:
        result?.columns.findIndex(
          (entry) => entry.name === resultColumnName(column)
        ) ?? -1,
    }));
    return (result?.rows ?? []).flatMap((row) => {
      const rowId = row[rowIdIndex];
      return typeof rowId === 'string'
        ? [
            {
              rowId,
              cells: Object.fromEntries(
                indexes.map(({ id, index }) => [id, row[index] ?? null])
              ),
            },
          ]
        : [];
    });
  }
  const retainedRows = () => {
    const ids = retainedRowIds();
    if (!ids.length) return [];
    return (rowsOf(retainedQuery, retainedStatement) ?? []).filter((row) =>
      ids.includes(row.rowId)
    );
  };
  const snapshot = () => {
    const rows = rowsOf(rowsQuery, viewStatement);
    if (!rows) return undefined;
    return { version: readVersion(), rows, retained: retainedRows() };
  };

  /** Read from the network again; what comes back is at least `version`. */
  async function readAgain(version: number) {
    await Promise.all([
      rowsQuery.refresh(),
      retainedRowIds().length ? retainedQuery.refresh() : undefined,
    ]);
    setReadVersion((previous) => Math.max(previous, version));
  }
  async function refresh() {
    const failed = rowsQuery.error();
    if (isSqlError(failed)) {
      staleSchemaError = failed;
      await refreshSchema();
    }
    await readAgain(currentTable().table.version);
  }
  // Another viewer's edit. This writer's own edits read their version back.
  props.onTableChanged((version) => {
    if (version <= readVersion()) return;
    refreshInBackground({
      refresh: () => readAgain(Math.max(version, currentTable().table.version)),
    });
  });

  function columnForWrite(table: DatabaseTableDetail, columnId: string) {
    const column = table.columns.find(
      (column) => column.column.id === columnId
    );
    if (!column?.writable) throw new Error('This property is read-only.');
    return column;
  }

  function statement(mutation: DatabaseRowMutation) {
    // Read the refreshed cache directly; Solid props may notify after fetchQuery resolves.
    const table = currentTable();
    const tableSqlName = table.sql_name;
    if (mutation.kind === 'cell') {
      const column = columnForWrite(table, mutation.columnId);
      return updateCellStatement({
        tableSqlName,
        rowId: mutation.rowId,
        columnSqlName: column.sql_name,
        value: writeValue(column, mutation.value),
      });
    }
    if (mutation.kind === 'delete')
      return deleteRowStatement({ tableSqlName, rowId: mutation.rowId });
    return insertRowStatement({
      tableSqlName,
      values: Object.fromEntries(
        Object.entries(mutation.values).map(([id, value]) => {
          const column = columnForWrite(table, id);
          return [column.sql_name, writeValue(column, value)];
        })
      ),
    });
  }

  async function prepareFirstValues(
    mutation: DatabaseRowMutation,
    baseVersion: number | undefined
  ) {
    let version = baseVersion;
    while (version !== undefined && inferredVersions.has(version))
      version = inferredVersions.get(version);
    if (mutation.kind === 'delete') return { mutation, version };
    const values =
      mutation.kind === 'cell'
        ? { [mutation.columnId]: mutation.value }
        : { ...mutation.values };
    for (const [columnId, value] of Object.entries(values)) {
      if (value === null || value === '') continue;
      let column = columnForWrite(currentTable(), columnId);
      if (column.column.config?.kind === 'link') continue;
      const requested = mutation.columnTypes?.[columnId];
      if (column.column.infer_type) {
        if (version === undefined)
          throw new Error(
            'Refresh this table before entering its first value.'
          );
        const numeric =
          typeof value === 'number' ? value : inferDatabaseNumber(value);
        const type: DatabaseColumnType = requested ?? {
          dataType: numeric === undefined ? 'STRING' : 'NUMBER',
        };
        const result = await storageServiceClient.databases.inferColumnType({
          id: props.databaseId,
          tableId,
          columnId,
          request: {
            data_type: type.dataType,
            ...(type.dataType === 'ENTITY'
              ? { specific_entity_type: type.entityType }
              : {}),
            base_version: version,
          },
        });
        if (result.isErr()) {
          const competing = result.error.some(
            (error) => error.code === 'VERSION_CONFLICT'
          );
          try {
            await refreshSchema();
          } catch {
            // Preserve the rejected entry and its actual validation error.
          }
          // Another first entry typed this column meanwhile: write against it.
          const refreshed = columnForWrite(currentTable(), columnId);
          if (competing && !refreshed.column.infer_type) {
            column = refreshed;
            version = currentTable().table.version;
          } else
            throw new Error(
              result.error[0]?.message ??
                'Could not set the column type. Your entry is kept.'
            );
        } else {
          column = result.value.column;
          inferredVersions.set(version, result.value.table_version);
          version = result.value.table_version;
          await queryClient.cancelQueries({ queryKey: detailKey, exact: true });
          queryClient.setQueryData(
            detailKey,
            (previous: DatabaseDetail | undefined) =>
              previous && {
                ...previous,
                tables: previous.tables.map((entry) =>
                  entry.table.id === tableId &&
                  entry.table.version <= result.value.table_version
                    ? {
                        ...entry,
                        table: {
                          ...entry.table,
                          version: result.value.table_version,
                        },
                        columns: entry.columns.map((existing) =>
                          existing.column.id === columnId
                            ? result.value.column
                            : existing
                        ),
                      }
                    : entry
                ),
              }
          );
        }
      }
      const definition = column.definition.definition;
      if (
        requested?.dataType === 'ENTITY' &&
        (definition.data_type !== 'ENTITY' ||
          definition.specific_entity_type !== requested.entityType)
      )
        throw new Error(
          'This column has a different type. Choose a matching mention or add a new column.'
        );
      if (definition.data_type === 'NUMBER' && typeof value === 'string') {
        const numeric = inferDatabaseNumber(value);
        if (numeric === undefined)
          throw new Error(
            'This column expects a number. Your entry is kept so you can correct it.'
          );
        values[columnId] = numeric;
      }
    }
    return {
      mutation:
        mutation.kind === 'cell'
          ? { ...mutation, value: values[mutation.columnId] }
          : { ...mutation, values },
      version,
    };
  }

  return {
    columns,
    snapshot,
    loading: () => !rowsQuery.outcome() && rowsQuery.error() === undefined,
    refreshing: rowsQuery.loading,
    error: () => readError(rowsQuery.error()),
    refresh,
    addOption: props.addOption,
    retain: (rowIds) => setRetainedIds(() => rowIds),
    write: async (mutation, version) => {
      const previousSchemaError = staleSchemaError;
      if (previousSchemaError) {
        try {
          await refreshSchema();
        } catch {
          throw previousSchemaError;
        }
      }
      try {
        const prepared = await prepareFirstValues(mutation, version);
        // Row writes are last-write-wins; live updates keep viewers current.
        const request = { sql: statement(prepared.mutation) };
        let written: ExecOutcome;
        try {
          written = await props.exec(request);
        } catch (error) {
          const code =
            error instanceof Error && 'code' in error ? error.code : undefined;
          if (
            mutation.kind === 'create' &&
            ![
              'SQL_ERROR',
              'READ_ONLY',
              'VERSION_CONFLICT',
              'BUDGET_EXCEEDED',
              'UNAUTHORIZED',
              'FORBIDDEN',
              'NOT_FOUND',
              'GONE',
              'CONFLICT',
            ].includes(String(code))
          )
            throw new DatabaseWriteOutcomeUnknown(
              'This row may already be saved. Check the latest rows before creating it again. Your draft is kept here.'
            );
          throw error;
        }
        props.applyVersions(written.new_versions);
        return {
          insertedRowIds: written.inserted_row_ids,
          version: written.new_versions[tableId],
        };
      } catch (error) {
        if (isSqlError(error)) {
          staleSchemaError = error;
          try {
            await refreshSchema();
          } catch {
            // Keep the original failed edit; the next write must refresh first.
          }
        }
        throw error;
      }
    },
  };
}
