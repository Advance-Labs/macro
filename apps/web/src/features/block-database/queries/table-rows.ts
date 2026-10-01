import { databaseSqlAnswer } from '@core/database-sql/answer';
import { databaseSqlSchema } from '@core/database-sql/catalog';
import { DatabaseSqlError } from '@core/database-sql/driver';
import type { DatabaseOp, OpResult } from '@core/database-sql/generated/types';
import { throwOnErr } from '@core/util/result';
import {
  createDatabaseSqlQuery,
  type DatabaseSqlQuery,
  type DatabaseSqlQueryCapabilities,
  type DatabaseSqlStatement,
  refreshInBackground,
  sameDatabaseSqlStatement,
} from '@queries/database-sql/create-database-sql-query';
import { DatabaseOpsError } from '@queries/storage/databases';
import { databasesKeys } from '@queries/storage/keys';
import { storageServiceClient } from '@service-storage/client';
import type {
  DatabaseColumnDetail,
  DatabaseDetail,
  DatabaseTableDetail,
} from '@service-storage/databases';
import { useQueryClient } from '@tanstack/solid-query';
import { type Accessor, createMemo, createSignal, untrack } from 'solid-js';
import {
  type DatabaseRowsSource,
  DatabaseWriteOutcomeUnknown,
} from '../context/table-source';
import { mutationOp } from '../core/cell-ops';
import {
  type DatabaseColumnType,
  inferDatabaseNumber,
} from '../core/column-inference';
import type {
  DatabaseViewColumn,
  DatabaseViewConfig,
} from '../core/database-view';
import type { DatabaseRow, DatabaseRowMutation } from '../core/table';
import {
  ROW_ID_COLUMN,
  resultColumnName,
  rowsByIdStatement,
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

/** Lookup columns are not part of the grid, so a view never names them. */
function isGridColumn(column: DatabaseColumnDetail) {
  return column.column.config?.kind !== 'lookup';
}

/** A stale table or column name, which a refreshed schema may resolve. */
function isStaleSchemaError(error: unknown): error is Error {
  return (
    error instanceof DatabaseSqlError ||
    (error instanceof DatabaseOpsError && error.code === 'INVALID_OP')
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
  /** Applies a write's ops to this database; reads run in the browser's SQL engine. */
  applyOps: (ops: DatabaseOp[]) => Promise<OpResult[]>;
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
          schema: databaseSqlSchema([{ ...detail, tables: [table] }]),
          scope: props.databaseId,
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

  function rowsOf(query: DatabaseSqlQuery): DatabaseRow[] | undefined {
    const outcome = query.outcome();
    const catalog = query.catalog();
    if (!outcome || !catalog) return undefined;
    const result = databaseSqlAnswer(outcome, catalog, []).results[0];
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
    return (rowsOf(retainedQuery) ?? []).filter((row) =>
      ids.includes(row.rowId)
    );
  };
  const snapshot = () => {
    const rows = rowsOf(rowsQuery);
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
    if (isStaleSchemaError(failed)) {
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
    write: async (mutation, version, createOptions) => {
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
        // Read the refreshed cache directly; Solid props may notify after fetchQuery resolves.
        const table = currentTable();
        const op = mutationOp(
          tableId,
          prepared.mutation,
          (columnId) => columnForWrite(table, columnId),
          createOptions
        );
        let written: OpResult | undefined;
        try {
          [written] = await props.applyOps([op]);
        } catch (error) {
          if (
            mutation.kind === 'create' &&
            !(error instanceof DatabaseOpsError && error.definite)
          )
            throw new DatabaseWriteOutcomeUnknown(
              'This row may already be saved. Check the latest rows before creating it again. Your draft is kept here.'
            );
          throw error;
        }
        if (written?.kind !== 'rows_written')
          throw new Error(
            'The database answered the edit with something else.'
          );
        props.applyVersions({ [tableId]: written.tableVersion });
        // New options live in the schema; read it again in the background
        // so they show as options without suspending the grid.
        if (createOptions)
          void queryClient.invalidateQueries({
            queryKey: detailKey,
            exact: true,
          });
        return {
          insertedRowIds: written.inserted,
          version: written.tableVersion,
        };
      } catch (error) {
        if (isStaleSchemaError(error)) {
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
