import { databaseSqlSchema } from '@core/database-sql/catalog';
import type { DatabaseOp } from '@core/database-sql/generated/types';
import { type ResultError, thrownResultErrorHasCode } from '@core/util/result';
import {
  createDatabaseSqlQuery,
  type DatabaseSqlQuery,
  type DatabaseSqlQueryCapabilities,
  type DatabaseSqlStatement,
  refreshInBackground,
  sameDatabaseSqlStatement,
} from '@queries/database-sql/create-database-sql-query';
import { databaseDetailQueryOptions } from '@queries/storage/databases';
import { databasesKeys } from '@queries/storage/keys';
import { storageServiceClient } from '@service-storage/client';
import type {
  DatabaseOpsError,
  DatabaseSchemaErrorCode,
} from '@service-storage/databases';
import type { ColumnDetail } from '@service-storage/generated/schemas/columnDetail';
import type { DatabaseDetail } from '@service-storage/generated/schemas/databaseDetail';
import type { DatabaseView } from '@service-storage/generated/schemas/databaseView';
import type { OpResult } from '@service-storage/generated/schemas/opResult';
import type { TableDetail } from '@service-storage/generated/schemas/tableDetail';
import { useQueryClient } from '@tanstack/solid-query';
import {
  err,
  errAsync,
  ok,
  okAsync,
  type Result,
  ResultAsync,
} from 'neverthrow';
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
import type {
  DatabaseCellValue,
  DatabaseViewColumn,
} from '../core/database-view';
import { gridRows } from '../core/grid-cells';
import type { DatabaseRow, DatabaseRowMutation } from '../core/table';
import { noRowFilter, searchFilter } from '../core/view-query';
import type {
  DatabaseCellFailure,
  DatabaseReadFailure,
  DatabaseWriteFailure,
} from '../core/write-failure';
import { rowsByIdStatement } from '../sql';
import { patchTableColumn } from './detail-cache';

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

/** A missing or deleted database means the table is gone; anything else is a failed read. */
function schemaReadFailure(thrown: unknown): DatabaseReadFailure {
  return thrownResultErrorHasCode(thrown, 'NOT_FOUND') ||
    thrownResultErrorHasCode(thrown, 'GONE')
    ? TABLE_UNAVAILABLE
    : { kind: 'fetch', message: 'The table’s schema could not be read.' };
}

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
  /**
   * The table as the cached schema has it; undefined once the schema no
   * longer lists it. Before the schema is cached the prop is all there is.
   */
  const currentTable = (): TableDetail | undefined => {
    const detail = cachedDetail();
    if (!detail) return props.table();
    return detail.tables.find((entry) => entry.table.id === tableId);
  };
  // Reads are rebuilt from the cached schema: a refreshed table name must
  // reach the retry even when the table prop has not caught up.
  const [schemaRefreshes, setSchemaRefreshes] = createSignal(0);

  function refreshSchema(): ResultAsync<void, DatabaseReadFailure> {
    const fetched = async () => {
      await queryClient.cancelQueries({ queryKey: detailKey, exact: true });
      return queryClient.fetchQuery({
        ...databaseDetailQueryOptions(props.databaseId),
        staleTime: 0,
      });
    };
    return ResultAsync.fromPromise(fetched(), schemaReadFailure).andThen(
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
  const columns = createMemo(() => details().map(toViewColumn));
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
        const table = currentTable();
        if (!detail || !table) return undefined;
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
    const narrowing = match(
      searchFilter(props.search(), table.columns.map(toViewColumn))
    )
      .with(undefined, () => undefined)
      .with({ kind: 'matching' }, ({ filter }) => filter)
      // A table without columns has no cell to test; its rows are hidden instead.
      .with({ kind: 'nothing' }, () =>
        table.columns[0] ? noRowFilter(table.columns[0].column.id) : undefined
      )
      .exhaustive();
    if (!narrowing) return { view };
    const filter = view.query.filter;
    return {
      view: {
        ...view,
        query: {
          ...view.query,
          filter: {
            conjunction: 'and',
            conditions: filter
              ? [{ kind: 'group', ...filter }, narrowing]
              : [narrowing],
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
    untrack(() => (currentTable() ?? props.table()).table.version)
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
  // Searching a table without columns finds nothing, though its view keeps every row.
  const searchHidesEveryRow = () =>
    details().length === 0 &&
    searchFilter(props.search(), [])?.kind === 'nothing';
  const snapshot = () => {
    const rows = rowsOf(rowsQuery);
    if (!rows) return undefined;
    return {
      version: readVersion(),
      rows: searchHidesEveryRow() ? [] : rows,
      retained: retainedRows(),
    };
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
    return schema.andThen(() => {
      const table = currentTable();
      return table
        ? readAgain(table.table.version)
        : errAsync<void, DatabaseReadFailure>(TABLE_UNAVAILABLE);
    });
  }
  // Another viewer's edit. This writer's own edits read their version back.
  props.onTableChanged((version) => {
    if (version <= readVersion()) return;
    refreshInBackground({
      refresh: () =>
        readAgain(Math.max(version, currentTable()?.table.version ?? version)),
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

  /** The base a new column's type is settled against, past the settlements this writer made from it. */
  function latestInferenceBase(
    inferenceBaseVersion: number | undefined
  ): number | undefined {
    let base = inferenceBaseVersion;
    while (base !== undefined && inferredVersions.has(base))
      base = inferredVersions.get(base);
    return base;
  }

  /**
   * Settle a new column's type from its first value. Another first entry
   * may have typed it meanwhile; then the value is written against that type.
   */
  async function settleColumnType(
    columnId: string,
    type: DatabaseColumnType,
    base: number
  ): Promise<
    Result<{ column: ColumnDetail; base: number }, DatabaseWriteFailure>
  > {
    const inferred = await storageServiceClient.databases.inferColumnType({
      id: props.databaseId,
      tableId,
      columnId,
      request: {
        dataType: type.dataType,
        ...(type.dataType === 'ENTITY'
          ? { specificEntityType: type.entityType }
          : {}),
        baseVersion: base,
      },
    });
    if (inferred.isOk()) {
      const settled = inferred.value;
      inferredVersions.set(base, settled.table_version);
      await patchTableColumn(queryClient, {
        databaseId: props.databaseId,
        tableId,
        columnId,
        tableVersion: settled.table_version,
        change: () => settled.column,
      });
      return ok({ column: settled.column, base: settled.table_version });
    }
    const competing = inferred.error.some((error) => error.code === 'CONFLICT');
    // A failed refresh keeps the rejected entry and its own error.
    await refreshSchema();
    const table = currentTable();
    if (!table) return err(TABLE_UNAVAILABLE);
    const refreshed = columnForWrite(table, columnId);
    if (refreshed.isErr()) return err(refreshed.error);
    if (!competing || refreshed.value.column.infer_type)
      return err({ kind: 'type-refused', errors: inferred.error });
    return ok({ column: refreshed.value, base: table.table.version });
  }

  /** A first value as its column takes it: a number column's text read as a number. */
  function fittedValue(
    column: ColumnDetail,
    value: string | number,
    numeric: number | undefined,
    requested: DatabaseColumnType | undefined
  ): Result<DatabaseCellValue, DatabaseWriteFailure> {
    const definition = column.definition.definition;
    if (
      requested?.dataType === 'ENTITY' &&
      (definition.data_type !== 'ENTITY' ||
        definition.specific_entity_type !== requested.entityType)
    )
      return err({ kind: 'type-mismatch' });
    if (definition.data_type !== 'NUMBER') return ok(value);
    return numeric === undefined ? err({ kind: 'not-a-number' }) : ok(numeric);
  }

  /** Settle the type of new columns from their first value, then fit values to their columns. */
  async function prepareFirstValues(
    mutation: DatabaseRowMutation,
    inferenceBaseVersion: number | undefined
  ): Promise<Result<DatabaseRowMutation, DatabaseWriteFailure>> {
    if (mutation.kind === 'delete') return ok(mutation);
    let base = latestInferenceBase(inferenceBaseVersion);
    const values =
      mutation.kind === 'cell'
        ? { [mutation.columnId]: mutation.value }
        : { ...mutation.values };
    for (const [columnId, value] of Object.entries(values)) {
      if (value === null || value === '') continue;
      const table = currentTable();
      if (!table) return err(TABLE_UNAVAILABLE);
      const found = columnForWrite(table, columnId);
      if (found.isErr()) return err(found.error);
      let column = found.value;
      if (column.column.config?.kind === 'link') continue;
      const requested = mutation.columnTypes?.[columnId];
      const numeric =
        typeof value === 'number' ? value : inferDatabaseNumber(value);
      if (column.column.infer_type) {
        if (base === undefined) return err({ kind: 'needs-refresh' });
        const settled = await settleColumnType(
          columnId,
          requested ?? {
            dataType: numeric === undefined ? 'STRING' : 'NUMBER',
          },
          base
        );
        if (settled.isErr()) return err(settled.error);
        column = settled.value.column;
        base = settled.value.base;
      }
      const fitted = fittedValue(column, value, numeric, requested);
      if (fitted.isErr()) return err(fitted.error);
      values[columnId] = fitted.value;
    }
    return ok(
      mutation.kind === 'cell'
        ? { ...mutation, value: values[mutation.columnId] }
        : { ...mutation, values }
    );
  }

  async function writeRows(
    mutation: DatabaseRowMutation,
    inferenceBaseVersion: number | undefined,
    createOptions: boolean
  ): Promise<Result<DatabaseWriteResult, DatabaseWriteFailure>> {
    const prepared = await prepareFirstValues(mutation, inferenceBaseVersion);
    if (prepared.isErr()) return err(prepared.error);
    // Read the refreshed cache directly; Solid props may notify after fetchQuery resolves.
    const table = currentTable();
    if (!table) return err(TABLE_UNAVAILABLE);
    const op = mutationOp(
      tableId,
      prepared.value,
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
    inferenceBaseVersion: number | undefined,
    createOptions: boolean
  ): Promise<Result<DatabaseWriteResult, DatabaseWriteFailure>> {
    if (schemaStale) {
      const refreshed = await refreshSchema();
      if (refreshed.isErr())
        return err(
          refreshed.error.kind === 'table-unavailable'
            ? TABLE_UNAVAILABLE
            : { kind: 'schema-unreachable' }
        );
    }
    const written = await writeRows(
      mutation,
      inferenceBaseVersion,
      createOptions
    );
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
    write: (mutation, inferenceBaseVersion, createOptions) =>
      new ResultAsync(write(mutation, inferenceBaseVersion, createOptions)),
  };
}
