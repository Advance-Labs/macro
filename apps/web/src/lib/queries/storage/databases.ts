/**
 * Server state for Macro Databases.
 *
 * Schema reads go through `GET /databases/{id}`; row writes are typed ops
 * through `POST /databases/{id}/ops`. Row reads run in the browser's SQL
 * engine over Soup (`@queries/database-sql`).
 */
import { analytics } from '@app/lib/analytics';
import { useFeatureFlag } from '@app/lib/analytics/posthog';
import { enableDatabases, isFeatureEnabled } from '@core/constant/featureFlags';
import type { DatabaseOp, OpResult } from '@core/database-sql/generated/types';
import { type ResultError, throwOnErr } from '@core/util/result';
import { storageServiceClient } from '@service-storage/client';
import type {
  DatabaseOpsError,
  DatabaseSchemaErrorCode,
} from '@service-storage/databases';
import type { ColumnDetail } from '@service-storage/generated/schemas/columnDetail';
import type { CreateColumnRequest } from '@service-storage/generated/schemas/createColumnRequest';
import type { DatabaseDetail } from '@service-storage/generated/schemas/databaseDetail';
import type { ListedDatabase } from '@service-storage/generated/schemas/listedDatabase';
import { useQueries, useQuery } from '@tanstack/solid-query';
import { okAsync, type ResultAsync } from 'neverthrow';
import type { Accessor } from 'solid-js';
import { queryClient } from '../client';
import { databasesKeys } from './keys';

const DATABASE_STALE_TIME = 30 * 1000;

const databaseListQueryOptions = {
  queryKey: databasesKeys.list.queryKey,
  queryFn: (): Promise<ListedDatabase[]> =>
    throwOnErr(() => storageServiceClient.databases.list()),
  staleTime: DATABASE_STALE_TIME,
};

export function useDatabasesQuery() {
  const flag = useFeatureFlag(enableDatabases);
  return useQuery(() => ({
    ...databaseListQueryOptions,
    enabled: flag().enabled,
  }));
}

function databaseDetailQueryOptions(id: string) {
  return {
    queryKey: databasesKeys.detail(id).queryKey,
    queryFn: (): Promise<DatabaseDetail> =>
      throwOnErr(() => storageServiceClient.databases.get({ id })),
    staleTime: DATABASE_STALE_TIME,
  };
}

export function useDatabaseDetailQuery(databaseId: () => string | undefined) {
  return useQuery(() => {
    const id = databaseId();
    return { ...databaseDetailQueryOptions(id ?? ''), enabled: !!id };
  });
}

function liveDatabaseIds(listed: readonly ListedDatabase[]): string[] {
  return listed
    .filter((entry) => !entry.database.trashed_at)
    .map((entry) => entry.database.id);
}

/**
 * Every database the viewer can reach, in detail: what the server builds a
 * statement's catalog from when it may read any of them.
 */
export function useViewerDatabases(): {
  databases: Accessor<DatabaseDetail[] | undefined>;
  error: Accessor<unknown>;
} {
  const list = useDatabasesQuery();
  const details = useQueries(() => ({
    queries: (list.isSuccess ? liveDatabaseIds(list.data) : []).map(
      databaseDetailQueryOptions
    ),
  }));
  return {
    databases: () =>
      list.isSuccess && details.every((detail) => detail.isSuccess)
        ? details.map((detail) => detail.data)
        : undefined,
    error: () =>
      list.isError
        ? list.error
        : details.find((detail) => detail.isError)?.error,
  };
}

/** {@link useViewerDatabases}, read once. */
export async function fetchViewerDatabases(): Promise<DatabaseDetail[]> {
  const listed = await queryClient.fetchQuery(databaseListQueryOptions);
  return Promise.all(
    liveDatabaseIds(listed).map((id) =>
      queryClient.fetchQuery(databaseDetailQueryOptions(id))
    )
  );
}

/** The batch's refusal; the service answers with one error. */
function firstOpsError(errors: DatabaseOpsError[]): DatabaseOpsError {
  return (
    errors[0] ?? {
      code: 'UNKNOWN_ERROR',
      message: 'The database could not apply that change.',
      refusal: null,
    }
  );
}

/**
 * Apply ops to one database's rows as the current viewer, together or not at
 * all: the browser's `OpsSink.apply`.
 */
export function applyDatabaseOps(
  databaseId: string,
  ops: DatabaseOp[]
): ResultAsync<OpResult[], DatabaseOpsError> {
  return storageServiceClient.databases
    .applyOps({ id: databaseId, request: { ops } })
    .map((response) => response.results)
    .mapErr(firstOpsError);
}

/**
 * Re-read one database's schema. Open reads rerun when the catalog they are
 * built from changes, so a version-only change reruns nothing.
 */
export function invalidateDatabase(databaseId: string) {
  return queryClient.invalidateQueries({
    queryKey: databasesKeys.detail(databaseId).queryKey,
  });
}

/**
 * Fold the versions a write reported straight into the cached schema.
 *
 * The version is the only part of the detail response a write moves, so
 * patching it in place spares the grid a schema refetch it would otherwise
 * make on every cell edit. Delayed write responses cannot move the schema
 * version behind a newer refresh or acknowledged schema change.
 */
export function applyDatabaseTableVersions(
  databaseId: string,
  newVersions: Record<string, number>
) {
  if (Object.keys(newVersions).length === 0) return;

  queryClient.setQueryData(
    databasesKeys.detail(databaseId).queryKey,
    (previous: DatabaseDetail | undefined): DatabaseDetail | undefined => {
      if (!previous) return previous;
      return {
        ...previous,
        tables: previous.tables.map((table) => {
          const version = newVersions[table.table.id];
          if (version === undefined || version <= table.table.version) {
            return table;
          }
          return { ...table, table: { ...table.table, version } };
        }),
      };
    }
  );
}

/** Add a column to a table and return its id. */
export function createDatabaseColumn(params: {
  databaseId: string;
  tableId: string;
  request: CreateColumnRequest;
}): ResultAsync<string, ResultError<DatabaseSchemaErrorCode>[]> {
  return storageServiceClient.databases
    .createColumn({
      id: params.databaseId,
      tableId: params.tableId,
      request: params.request,
    })
    .map(async ({ columnId }) => {
      await invalidateDatabase(params.databaseId);
      return columnId;
    });
}

/**
 * Add select option labels to a column and return the column as it now stands.
 *
 * The updated column is folded into the cached schema rather than refetched,
 * so the dropdown that asked for the option can offer it on the very next
 * open; open reads rerun with the new option in their catalog.
 */
export function addDatabaseColumnOptions(params: {
  databaseId: string;
  tableId: string;
  columnId: string;
  labels: string[];
}): ResultAsync<ColumnDetail, ResultError<DatabaseSchemaErrorCode>[]> {
  return storageServiceClient.databases
    .addColumnOptions({
      id: params.databaseId,
      tableId: params.tableId,
      columnId: params.columnId,
      request: { labels: params.labels },
    })
    .map((updated) => {
      queryClient.setQueryData(
        databasesKeys.detail(params.databaseId).queryKey,
        (previous: DatabaseDetail | undefined): DatabaseDetail | undefined =>
          previous && {
            ...previous,
            tables: previous.tables.map((table) =>
              table.table.id === params.tableId
                ? {
                    ...table,
                    columns: table.columns.map((column) =>
                      column.column.id === params.columnId ? updated : column
                    ),
                  }
                : table
            ),
          }
      );
      return updated;
    });
}

/**
 * Create a database and return its id.
 *
 * The service seeds a starter table. Give it a Name column so the first row is
 * ready to edit. If setup fails, still open the created database: its empty
 * state lets the user add a column without creating another database.
 */
export async function createDatabase(params: {
  name: string;
  /** UI surface the creation originated from, for analytics. */
  source?: string;
}): Promise<string | undefined> {
  if (!isFeatureEnabled(enableDatabases)) return undefined;
  const created = await storageServiceClient.databases
    .create({ name: params.name })
    .andThen(({ id }) =>
      storageServiceClient.databases
        .get({ id })
        .andThen((detail) => {
          const starter = detail.tables[0];
          return starter && starter.columns.length === 0
            ? storageServiceClient.databases.createColumn({
                id,
                tableId: starter.table.id,
                request: {
                  binding: {
                    kind: 'new',
                    name: 'Name',
                    data_type: 'STRING',
                    is_multi_select: false,
                  },
                },
              })
            : okAsync(undefined);
        })
        // The database exists: its empty state lets the user add a column.
        .orElse(() => okAsync(undefined))
        .map(() => id)
    );
  if (created.isErr()) return undefined;
  const databaseId = created.value;
  analytics.track('create_entity', {
    entityType: 'database',
    entityId: databaseId,
    source: params.source,
  });
  await queryClient.invalidateQueries({
    queryKey: databasesKeys.list.queryKey,
  });
  return databaseId;
}

/** Native sharing adapters retain Result so the shared dialog can render failures. */
export function getDatabaseSharePermissions(id: string) {
  return storageServiceClient.databases.getPermissions({ id });
}

export function updateDatabaseSharePermissions(
  params: Parameters<typeof storageServiceClient.databases.updatePermissions>[0]
) {
  return storageServiceClient.databases.updatePermissions(params);
}
