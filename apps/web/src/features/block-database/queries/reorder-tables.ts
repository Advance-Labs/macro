import { queryClient } from '@queries/client';
import { invalidateDatabase } from '@queries/storage/databases';
import { databasesKeys } from '@queries/storage/keys';
import { storageServiceClient } from '@service-storage/client';
import type { DatabaseDetail } from '@service-storage/generated/schemas/databaseDetail';
import type { Table } from '@service-storage/generated/schemas/table';
import { ResultAsync } from 'neverthrow';
import type { DatabaseSchemaChange } from '../core/column-schema';

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

const writes = new Map<string, PromiseLike<unknown>>();

function tableOrderOf(detail: DatabaseDetail | undefined) {
  return detail?.tables.map((entry) => entry.table.id).join(',');
}

/**
 * Show the new tab order at once and persist it. On failure the cached order
 * goes back to what it was, unless a newer move has replaced it since, and the
 * database is refetched so a stale list of tables corrects itself.
 */
export function reorderDatabaseTables(params: {
  databaseId: string;
  tableIds: string[];
}): DatabaseSchemaChange {
  const key = databasesKeys.detail(params.databaseId).queryKey;
  const reorder = async () => {
    // An in-flight read must not paint the old order over the optimistic one.
    await queryClient.cancelQueries({ queryKey: key });
    const previous = queryClient.getQueryData<DatabaseDetail>(key);
    const optimistic = previous && withTableOrder(previous, params.tableIds);
    if (optimistic) queryClient.setQueryData(key, optimistic);

    // Requests go out in move order, so the last move is the one that sticks.
    const earlier = writes.get(params.databaseId);
    const send = async () => {
      await earlier;
      return storageServiceClient.databases.reorderTables({
        id: params.databaseId,
        tableIds: params.tableIds,
      });
    };
    const request = send();
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
      return result.map(() => undefined);
    }
    commit(key, result.value);
    return result.map(() => undefined);
  };
  return new ResultAsync(reorder());
}

function commit(key: readonly unknown[], tables: Table[]) {
  const committed = new Map(tables.map((table) => [table.id, table]));
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
