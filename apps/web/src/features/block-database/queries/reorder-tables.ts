import { queryClient } from '@queries/client';
import { invalidateDatabase } from '@queries/storage/databases';
import { databasesKeys } from '@queries/storage/keys';
import { storageServiceClient } from '@service-storage/client';
import type { DatabaseDetail } from '@service-storage/databases';

function withTableOrder(
  detail: DatabaseDetail,
  tableIds: string[]
): DatabaseDetail {
  const rank = new Map(tableIds.map((id, index) => [id, index]));
  return {
    ...detail,
    tables: detail.tables.toSorted(
      (a, b) =>
        (rank.get(a.table.id) ?? Number.MAX_SAFE_INTEGER) -
        (rank.get(b.table.id) ?? Number.MAX_SAFE_INTEGER)
    ),
  };
}

const writes = new Map<string, Promise<unknown>>();

function tableOrderOf(detail: DatabaseDetail | undefined) {
  return detail?.tables.map((entry) => entry.table.id).join(',');
}

/**
 * Show the new tab order at once and persist it. On failure the cached order
 * goes back to what it was, unless a newer move has replaced it since, and the
 * database is refetched so a stale list of tables corrects itself.
 */
export async function reorderDatabaseTables(params: {
  databaseId: string;
  tableIds: string[];
}): Promise<void> {
  const key = databasesKeys.detail(params.databaseId).queryKey;
  // An in-flight read must not paint the old order over the optimistic one.
  await queryClient.cancelQueries({ queryKey: key });
  const previous = queryClient.getQueryData<DatabaseDetail>(key);
  const optimistic = previous && withTableOrder(previous, params.tableIds);
  if (optimistic) queryClient.setQueryData(key, optimistic);

  // Requests go out in move order, so the last move is the one that sticks.
  const request = (writes.get(params.databaseId) ?? Promise.resolve()).then(
    () =>
      storageServiceClient.databases.reorderTables({
        id: params.databaseId,
        tableIds: params.tableIds,
      })
  );
  writes.set(params.databaseId, request);
  const result = await request;
  if (writes.get(params.databaseId) === request)
    writes.delete(params.databaseId);
  if (result.isErr()) {
    if (
      previous &&
      tableOrderOf(queryClient.getQueryData<DatabaseDetail>(key)) ===
        tableOrderOf(optimistic)
    )
      queryClient.setQueryData(key, previous);
    void invalidateDatabase(params.databaseId);
    throw new Error(
      'Could not move this table. The tables may have changed; try again.'
    );
  }
  const committed = new Map(result.value.map((table) => [table.id, table]));
  queryClient.setQueryData(key, (current: DatabaseDetail | undefined) =>
    current
      ? {
          ...current,
          tables: current.tables.map((entry) => {
            const table = committed.get(entry.table.id);
            return table && entry.table.version <= table.version
              ? { ...entry, table }
              : entry;
          }),
        }
      : current
  );
}
