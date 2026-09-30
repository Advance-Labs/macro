import { throwOnErr } from '@core/util/result';
import { databasesKeys } from '@queries/storage/keys';
import { storageServiceClient } from '@service-storage/client';
import type {
  DatabaseColumnDetail,
  DatabaseDetail,
  DatabaseTableDetail,
  ExecOutcome,
  ExecRequest,
} from '@service-storage/databases';
import {
  keepPreviousData,
  useQuery,
  useQueryClient,
} from '@tanstack/solid-query';
import { type Accessor, createMemo, createSignal } from 'solid-js';
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
  exec: (request: ExecRequest) => Promise<ExecOutcome>;
  applyVersions: (versions: Record<string, number>) => void;
  addOption: (columnId: string, label: string) => Promise<void>;
}): DatabaseRowsSource {
  const queryClient = useQueryClient();
  const tableId = props.table().table.id;
  const detailKey = databasesKeys.detail(props.databaseId).queryKey;
  let staleSchemaError: Error | undefined;
  // Only advance across schema changes this writer has itself acknowledged.
  const inferredVersions = new Map<number, number>();
  const currentTable = () =>
    queryClient
      .getQueryData<DatabaseDetail>(detailKey)
      ?.tables.find((entry) => entry.table.id === tableId) ?? props.table();

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
  }
  function isSqlError(error: unknown): error is Error {
    return (
      error instanceof Error && 'code' in error && error.code === 'SQL_ERROR'
    );
  }
  // Version-only schema updates must not recreate columns and remount editors.
  const details = createMemo(() => props.table().columns);
  const columns = createMemo(() =>
    details().filter(isGridColumn).map(toViewColumn)
  );
  const viewStatement = (table: DatabaseTableDetail) =>
    viewSelectStatement({
      tableSqlName: table.sql_name,
      columns: table.columns.filter(isGridColumn).map((column) => ({
        column: toViewColumn(column),
        sqlName: column.sql_name,
      })),
      view: props.view(),
    });
  const readStatement = createMemo(() => viewStatement(props.table()));
  const rowsKey = databasesKeys.rows(props.databaseId, tableId).queryKey;
  const query = useQuery(() => ({
    queryKey: [...rowsKey, readStatement()],
    // Build from the cached schema: a refreshed table name must reach the retry.
    queryFn: () => props.exec({ sql: viewStatement(currentTable()) }),
    // A changed search or filter keeps the grid on screen until its rows arrive.
    placeholderData: keepPreviousData,
  }));
  const [retainedIds, setRetainedIds] = createSignal<
    Accessor<readonly string[]>
  >(() => []);
  const retainedRowIds = createMemo(
    () => [...new Set(retainedIds()())].sort(),
    [],
    { equals: sameIds }
  );
  const retainedQuery = useQuery(() => ({
    queryKey: [...rowsKey, 'retained', retainedRowIds()],
    queryFn: () =>
      props.exec({
        sql: rowsByIdStatement(currentTable().sql_name, retainedRowIds()),
      }),
    enabled: retainedRowIds().length > 0,
    placeholderData: keepPreviousData,
  }));
  // Accepted draft writes can outlive the query observer's owner. Retain actual
  // reads so a post-unmount option change supplies its version to the next write.
  const [refreshedOutcome, setRefreshedOutcome] = createSignal<{
    statement: string;
    outcome: ExecOutcome;
  }>();
  function newerRead(
    current: ExecOutcome | undefined,
    candidate: ExecOutcome | undefined
  ) {
    if (!current) return candidate;
    if (!candidate) return current;
    return (candidate.read_versions[tableId] ?? -1) >
      (current.read_versions[tableId] ?? -1)
      ? candidate
      : current;
  }
  // Status reads are safe outside Suspense. data is read only after initial load.
  const outcome = () => {
    const refreshed = refreshedOutcome();
    return newerRead(
      !query.isPending ? query.data : undefined,
      refreshed?.statement === readStatement() ? refreshed.outcome : undefined
    );
  };
  function rowsOf(data: ExecOutcome): DatabaseRow[] {
    const result = data.results[0];
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
    if (!ids.length || retainedQuery.isPending || !retainedQuery.data)
      return [];
    return rowsOf(retainedQuery.data).filter((row) => ids.includes(row.rowId));
  };
  const snapshot = () => {
    const data = outcome();
    if (!data) return undefined;
    return {
      version: data.read_versions[props.table().table.id],
      rows: rowsOf(data),
      retained: retainedRows(),
    };
  };

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
    loading: () => query.isPending,
    refreshing: () => query.isFetching,
    error: () => (query.isError ? query.error : undefined),
    refresh: async () => {
      if (query.isError && isSqlError(query.error)) {
        staleSchemaError = query.error;
        await refreshSchema();
      }
      const read = readStatement();
      const [result] = await Promise.all([
        query.refetch({ throwOnError: true }),
        retainedRowIds().length
          ? retainedQuery.refetch({ throwOnError: true })
          : undefined,
      ]);
      setRefreshedOutcome((previous) => {
        const outcome = newerRead(
          previous?.statement === read ? previous.outcome : undefined,
          result.data
        );
        return outcome && { statement: read, outcome };
      });
    },
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
