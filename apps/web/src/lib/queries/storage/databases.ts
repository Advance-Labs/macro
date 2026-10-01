/** Database schemas and the typed ops that change them and their rows; row reads run in `@queries/database-sql`. */
import { analytics } from '@app/lib/analytics';
import { useFeatureFlag } from '@app/lib/analytics/posthog';
import { enableDatabases } from '@core/constant/featureFlags';
import type { DatabaseOp } from '@core/database-sql/generated/types';
import { catchToResult, type ResultError, throwOnErr } from '@core/util/result';
import { storageServiceClient } from '@service-storage/client';
import type {
  DatabaseOpsError,
  DatabaseSchemaErrorCode,
} from '@service-storage/databases';
import type { DatabaseDetail } from '@service-storage/generated/schemas/databaseDetail';
import type { ListedDatabase } from '@service-storage/generated/schemas/listedDatabase';
import type { OpResult } from '@service-storage/generated/schemas/opResult';
import { useQueries, useQuery } from '@tanstack/solid-query';
import { ResultAsync } from 'neverthrow';
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

/** One database's schema; spread it and override what a read needs differently. */
export function databaseDetailQueryOptions(id: string) {
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
    .filter((entry) => entry.database.trashed_at === null)
    .map((entry) => entry.database.id);
}

/** Every database the viewer can reach, in detail, for a statement that may read any of them. */
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

/** A cached read; the query's thrown errors come back as the client's own. */
function fetchCached<Data>(options: {
  queryKey: readonly unknown[];
  queryFn: () => Promise<Data>;
  staleTime: number;
}): ResultAsync<Data, ResultError[]> {
  return new ResultAsync(catchToResult(() => queryClient.fetchQuery(options)));
}

/** {@link useViewerDatabases}, read once. */
export function fetchViewerDatabases(): ResultAsync<
  DatabaseDetail[],
  ResultError[]
> {
  return fetchCached(databaseListQueryOptions).andThen((listed) =>
    ResultAsync.combine(
      liveDatabaseIds(listed).map((id) =>
        fetchCached(databaseDetailQueryOptions(id))
      )
    )
  );
}

/**
 * Apply ops to one database as the current viewer, together or not at all.
 * Each table `baseVersions` names must still be at that version, or the batch
 * is refused as a `CONFLICT`.
 */
export function applyDatabaseOps(
  databaseId: string,
  ops: DatabaseOp[],
  baseVersions?: Record<string, number>
): ResultAsync<OpResult[], DatabaseOpsError> {
  return (
    storageServiceClient.databases
      .applyOps({
        id: databaseId,
        request: baseVersions ? { ops, baseVersions } : { ops },
      })
      .map((response) => response.results)
      // A refused batch is one error: the first op the service could not apply.
      .mapErr(([refusal]) => refusal)
  );
}

/** Re-read one database's schema; open reads rerun only when their catalog changes. */
export function invalidateDatabase(databaseId: string) {
  return queryClient.invalidateQueries({
    queryKey: databasesKeys.detail(databaseId).queryKey,
  });
}

/**
 * Fold the versions a write reported into the cached schema, sparing a refetch per cell edit.
 * A delayed response never moves a version backwards.
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

/** Create a database; the service gives it a first table with a Name column. */
export function createDatabase(params: {
  name: string;
  /** UI surface the creation originated from, for analytics. */
  source?: string;
}): ResultAsync<string, ResultError<DatabaseSchemaErrorCode>[]> {
  return storageServiceClient.databases
    .create({ name: params.name })
    .map(async ({ id }) => {
      analytics.track('create_entity', {
        entityType: 'database',
        entityId: id,
        source: params.source,
      });
      await queryClient.invalidateQueries({
        queryKey: databasesKeys.list.queryKey,
      });
      return id;
    });
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
