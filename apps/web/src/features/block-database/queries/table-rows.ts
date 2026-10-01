import { databaseSqlSchema } from '@core/database-sql/catalog';
import type {
  DatabaseOp,
  DatabaseView,
  OpResult,
} from '@core/database-sql/generated/types';
import { type ResultError, throwOnErr } from '@core/util/result';
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
  DatabaseOpsError,
  DatabaseSchemaErrorCode,
} from '@service-storage/databases';
import type { ColumnDetail } from '@service-storage/generated/schemas/columnDetail';
import type { DatabaseDetail } from '@service-storage/generated/schemas/databaseDetail';
import type { TableDetail } from '@service-storage/generated/schemas/tableDetail';
import { useQueryClient } from '@tanstack/solid-query';
import { err, ok, okAsync, type Result, ResultAsync } from 'neverthrow';
import { type Accessor, createMemo, createSignal, untrack } from 'solid-js';
import { match, P } from 'ts-pattern';
import type {
  DatabaseRowsSource,
  DatabaseWriteResult,
} from '../context/table-source';
import { mutationOp } from '../core/cell-ops';
import {
  type DatabaseColumnType,
  inferDatabaseNumber,
} from '../core/column-inference';
import type { DatabaseViewColumn } from '../core/database-view';
import { gridRows } from '../core/grid-cells';
import type { DatabaseRow, DatabaseRowMutation } from '../core/table';
import { searchFilter } from '../core/view-query';
import type {
  DatabaseCellFailure,
  DatabaseReadFailure,
  DatabaseWriteFailure,
} from '../core/write-failure';
import { rowsByIdStatement } from '../sql';

export function toViewColumn(column: ColumnDetail): DatabaseViewColumn {
  const relation =
    column.column.config?.kind === 'link' ? column.column.config : undefined;
  return {
    id: column.column.id,
    name:
      column.column.display_name ?? column.definition.definition.display_name,
    dataType: column.definition.definition.data_type,
    isMultiSelect: !!relation || column.definition.definition.is_multi_select,
    options: column.definition.property_options.map((option) => ({
      id: option.id,
      label: String(option.value.value),
      color: option.color,
    })),
    writable: column.writable,
    sharedOutsideDatabase: column.shared_outside_database,
    ...(relation
      ? {
          relation: {
            databaseId: relation.database_id,
            tableId: relation.table_id,
          },
        }
      : {}),
    specificEntityType: column.definition.definition.specific_entity_type,
    inferType: column.column.infer_type,
  };
}

/** Lookup columns are not part of the grid, so a view never names them. */
function isGridColumn(column: ColumnDetail) {
  return column.column.config?.kind !== 'lookup';
}

/** A stale table or column name, which a refreshed schema may resolve. */
function isStaleSchema(
  failure: DatabaseReadFailure | DatabaseWriteFailure
): boolean {
  return (
    failure.kind === 'engine' ||
    (failure.kind === 'ops' && failure.error.code === 'INVALID_OP')
  );
}

/** The service answered and refused: the write certainly did not land. */
function isDefiniteRefusal(error: DatabaseOpsError): boolean {
  return match(error.code)
    .with(
      P.union(
        'INVALID_OP',
        'UNAUTHORIZED',
        'FORBIDDEN',
        'NOT_FOUND',
        'CONFLICT',
        'GONE'
      ),
      () => true
    )
    .otherwise(() => false);
}

const TABLE_UNAVAILABLE = { kind: 'table-unavailable' } as const;

function sameIds(left: readonly string[], right: readonly string[]) {
  return (
    left.length === right.length &&
    left.every((id, index) => id === right[index])
  );
}

export function createDatabaseRowsSource(props: {
  databaseId: string;
  table: Accessor<TableDetail>;
  /** The view whose rows the engine reads, filtered and sorted as it says. */
  view: Accessor<DatabaseView>;
  /** Rows also hold this text, in a text cell or an option's label. */
  search: Accessor<string>;
  /** Applies a write's ops to this database; reads run in the browser's SQL engine. */
  applyOps: (ops: DatabaseOp[]) => ResultAsync<OpResult[], DatabaseOpsError>;
  /** Where the engine reads rows from; the app's GraphQL client by default. */
  read?: DatabaseSqlQueryCapabilities;
  /** Calls back with the version of each change the gateway reports for this table. */
  onTableChanged: (listener: (version: number) => void) => void;
  applyVersions: (versions: Record<string, number>) => void;
  addOption: (
    columnId: string,
    label: string
  ) => ResultAsync<void, ResultError<DatabaseSchemaErrorCode>[]>;
}): DatabaseRowsSource {
  const queryClient = useQueryClient();
  const tableId = props.table().table.id;
  const detailKey = databasesKeys.detail(props.databaseId).queryKey;
  let schemaStale = false;
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

  function refreshSchema(): ResultAsync<void, DatabaseReadFailure> {
    const fetched = async () => {
      await queryClient.cancelQueries({ queryKey: detailKey, exact: true });
      return queryClient.fetchQuery({
        queryKey: detailKey,
        queryFn: () =>
          throwOnErr(() =>
            storageServiceClient.databases.get({ id: props.databaseId })
          ),
        staleTime: 0,
        retry: false,
      });
    };
    return ResultAsync.fromPromise(fetched(), () => TABLE_UNAVAILABLE).andThen(
      (detail) => {
        if (!detail.tables.some((entry) => entry.table.id === tableId))
          return err(TABLE_UNAVAILABLE);
        schemaStale = false;
        setSchemaRefreshes((count) => count + 1);
        return ok(undefined);
      }
    );
  }
  // Version-only schema updates must not recreate columns and remount editors.
  const details = createMemo(() => props.table().columns);
  const columns = createMemo(() =>
    details().filter(isGridColumn).map(toViewColumn)
  );
  /** A read of this table alone, built from the cached schema. */
  const tableStatement = (
    read: (
      table: TableDetail
    ) => { sql: string } | { view: DatabaseView } | undefined
  ) =>
    createMemo(
      (): DatabaseSqlStatement | undefined => {
        props.table();
        schemaRefreshes();
        const detail = cachedDetail();
        if (!detail) return undefined;
        const table = currentTable();
        const statement = read(table);
        if (statement === undefined) return undefined;
        return {
          schema: databaseSqlSchema([{ ...detail, tables: [table] }]),
          scope: props.databaseId,
          ...statement,
        };
      },
      undefined,
      { equals: sameDatabaseSqlStatement }
    );
  const viewStatement = tableStatement((table) => {
    const view = props.view();
    const search = searchFilter(
      props.search(),
      table.columns.filter(isGridColumn).map(toViewColumn)
    );
    if (!search) return { view };
    const filter = view.query.filter;
    return {
      view: {
        ...view,
        query: {
          ...view.query,
          filter: {
            conjunction: 'and',
            conditions: filter
              ? [{ kind: 'group', ...filter }, search]
              : [search],
          },
        },
      },
    };
  });
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
      ? { sql: rowsByIdStatement(table.sql_name, retainedRowIds()) }
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
    return gridRows(outcome, catalog, props.table().columns);
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
  function readAgain(version: number): ResultAsync<void, DatabaseReadFailure> {
    return ResultAsync.combine([
      rowsQuery.refresh(),
      retainedRowIds().length ? retainedQuery.refresh() : okAsync(undefined),
    ]).map(() => {
      setReadVersion((previous) => Math.max(previous, version));
    });
  }
  function refresh(): ResultAsync<void, DatabaseReadFailure> {
    const failed = rowsQuery.error();
    const schema =
      failed && isStaleSchema(failed) ? refreshSchema() : okAsync(undefined);
    return schema.andThen(() => readAgain(currentTable().table.version));
  }
  // Another viewer's edit. This writer's own edits read their version back.
  props.onTableChanged((version) => {
    if (version <= readVersion()) return;
    refreshInBackground({
      refresh: () => readAgain(Math.max(version, currentTable().table.version)),
    });
  });

  function columnForWrite(
    table: TableDetail,
    columnId: string
  ): Result<ColumnDetail, DatabaseCellFailure> {
    const column = table.columns.find(
      (column) => column.column.id === columnId
    );
    return column?.writable ? ok(column) : err({ kind: 'read-only-column' });
  }

  /** Settle the type of new columns from their first value, then fit values to their columns. */
  async function prepareFirstValues(
    mutation: DatabaseRowMutation,
    baseVersion: number | undefined
  ): Promise<
    Result<
      { mutation: DatabaseRowMutation; version: number | undefined },
      DatabaseWriteFailure
    >
  > {
    let version = baseVersion;
    while (version !== undefined && inferredVersions.has(version))
      version = inferredVersions.get(version);
    if (mutation.kind === 'delete') return ok({ mutation, version });
    const values =
      mutation.kind === 'cell'
        ? { [mutation.columnId]: mutation.value }
        : { ...mutation.values };
    for (const [columnId, value] of Object.entries(values)) {
      if (value === null || value === '') continue;
      const found = columnForWrite(currentTable(), columnId);
      if (found.isErr()) return err(found.error);
      let column = found.value;
      if (column.column.config?.kind === 'link') continue;
      const requested = mutation.columnTypes?.[columnId];
      if (column.column.infer_type) {
        if (version === undefined) return err({ kind: 'needs-refresh' });
        const numeric =
          typeof value === 'number' ? value : inferDatabaseNumber(value);
        const type: DatabaseColumnType = requested ?? {
          dataType: numeric === undefined ? 'STRING' : 'NUMBER',
        };
        const inferred = await storageServiceClient.databases.inferColumnType({
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
        if (inferred.isErr()) {
          const competing = inferred.error.some(
            (error) => error.code === 'CONFLICT'
          );
          // A failed refresh keeps the rejected entry and its own error.
          await refreshSchema();
          const refreshed = columnForWrite(currentTable(), columnId);
          if (refreshed.isErr()) return err(refreshed.error);
          // Another first entry typed this column meanwhile: write against it.
          if (!competing || refreshed.value.column.infer_type)
            return err({
              kind: 'type-refused',
              error: inferred.error[0] ?? {
                code: 'UNKNOWN_ERROR',
                message: 'Could not set the column type.',
              },
            });
          column = refreshed.value;
          version = currentTable().table.version;
        } else {
          const settled = inferred.value;
          column = settled.column;
          inferredVersions.set(version, settled.table_version);
          version = settled.table_version;
          await queryClient.cancelQueries({ queryKey: detailKey, exact: true });
          queryClient.setQueryData(
            detailKey,
            (previous: DatabaseDetail | undefined) =>
              previous && {
                ...previous,
                tables: previous.tables.map((entry) =>
                  entry.table.id === tableId &&
                  entry.table.version <= settled.table_version
                    ? {
                        ...entry,
                        table: {
                          ...entry.table,
                          version: settled.table_version,
                        },
                        columns: entry.columns.map((existing) =>
                          existing.column.id === columnId
                            ? settled.column
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
        return err({ kind: 'type-mismatch' });
      if (definition.data_type === 'NUMBER' && typeof value === 'string') {
        const numeric = inferDatabaseNumber(value);
        if (numeric === undefined) return err({ kind: 'not-a-number' });
        values[columnId] = numeric;
      }
    }
    return ok({
      mutation:
        mutation.kind === 'cell'
          ? { ...mutation, value: values[mutation.columnId] }
          : { ...mutation, values },
      version,
    });
  }

  async function writeRows(
    mutation: DatabaseRowMutation,
    version: number | undefined,
    createOptions: boolean
  ): Promise<Result<DatabaseWriteResult, DatabaseWriteFailure>> {
    const prepared = await prepareFirstValues(mutation, version);
    if (prepared.isErr()) return err(prepared.error);
    // Read the refreshed cache directly; Solid props may notify after fetchQuery resolves.
    const table = currentTable();
    const op = mutationOp(
      tableId,
      prepared.value.mutation,
      (columnId) => columnForWrite(table, columnId),
      createOptions
    );
    if (op.isErr()) return err(op.error);
    const applied = await props.applyOps([op.value]);
    if (applied.isErr())
      return err(
        mutation.kind === 'create' && !isDefiniteRefusal(applied.error)
          ? { kind: 'outcome-unknown' }
          : { kind: 'ops', error: applied.error }
      );
    const [written] = applied.value;
    if (written?.kind !== 'rows_written')
      return err({ kind: 'unexpected-result' });
    props.applyVersions({ [tableId]: written.tableVersion });
    // New options live in the schema; read it again in the background
    // so they show as options without suspending the grid.
    if (createOptions)
      void queryClient.invalidateQueries({
        queryKey: detailKey,
        exact: true,
      });
    return ok({
      insertedRowIds: written.inserted,
      version: written.tableVersion,
    });
  }

  async function write(
    mutation: DatabaseRowMutation,
    version: number | undefined,
    createOptions: boolean
  ): Promise<Result<DatabaseWriteResult, DatabaseWriteFailure>> {
    if (schemaStale) {
      const refreshed = await refreshSchema();
      if (refreshed.isErr()) return err(TABLE_UNAVAILABLE);
    }
    const written = await writeRows(mutation, version, createOptions);
    if (written.isErr() && isStaleSchema(written.error)) {
      schemaStale = true;
      // The failed edit keeps its own error; the next write refreshes first.
      await refreshSchema();
    }
    return written;
  }

  return {
    columns,
    snapshot,
    read: () => {
      const outcome = rowsQuery.outcome();
      const catalog = rowsQuery.catalog();
      const statement = viewStatement();
      return outcome && catalog && statement?.view
        ? { outcome, catalog, view: statement.view }
        : undefined;
    },
    loading: () => !rowsQuery.outcome() && rowsQuery.error() === undefined,
    refreshing: rowsQuery.loading,
    error: rowsQuery.error,
    refresh,
    addOption: props.addOption,
    retain: (rowIds) => setRetainedIds(() => rowIds),
    write: (mutation, version, createOptions) =>
      new ResultAsync(write(mutation, version, createOptions)),
  };
}
