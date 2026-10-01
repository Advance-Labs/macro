import { queryClient } from '@queries/client';
import { databaseDetailQueryOptions } from '@queries/storage/databases';
import { databasesKeys } from '@queries/storage/keys';
import { storageServiceClient } from '@service-storage/client';
import type { DatabaseDetail } from '@service-storage/generated/schemas/databaseDetail';
import { okAsync, ResultAsync } from 'neverthrow';
import type { DatabaseSchemaChange } from '../core/column-schema';
import type { TableCreationResult } from '../core/table-creation';

/** Resume setup by table ID so retrying a partial create never creates another table. */
export function createTableWithName(params: {
  databaseId: string;
  name: string;
  existingTableId?: string;
}): DatabaseSchemaChange<TableCreationResult> {
  const queryKey = databasesKeys.detail(params.databaseId).queryKey;
  const refresh = () => {
    const fetched = async () => {
      await queryClient.cancelQueries({ queryKey, exact: true });
      return queryClient.fetchQuery({
        ...databaseDetailQueryOptions(params.databaseId),
        staleTime: 0,
      });
    };
    return ResultAsync.fromPromise(fetched(), () => 'unloaded' as const);
  };
  const hasName = (detail: DatabaseDetail, tableId: string) =>
    detail.tables
      .find(({ table }) => table.id === tableId)
      ?.columns.some(
        ({ definition }) =>
          definition.definition.display_name.toLocaleLowerCase() === 'name'
      ) ?? false;
  const created = params.existingTableId
    ? okAsync(params.existingTableId)
    : storageServiceClient.databases
        .createTable({ id: params.databaseId, name: params.name })
        .map((table) => table.id);
  return created.andThen((tableId) => {
    const named = params.existingTableId
      ? refresh().map((detail) => hasName(detail, tableId))
      : okAsync<boolean, 'unloaded'>(false);
    const setUp = async (): Promise<TableCreationResult> => {
      const existing = await named;
      if (existing.isErr()) return unready(tableId, false);
      if (existing.value) return { tableId, ready: true };
      const column = await storageServiceClient.databases.createColumn({
        id: params.databaseId,
        tableId,
        request: {
          binding: {
            kind: 'new',
            name: 'Name',
            data_type: 'STRING',
            is_multi_select: false,
          },
        },
      });
      if (column.isErr()) return unready(tableId, false);
      return (await refresh()).isOk()
        ? { tableId, ready: true }
        : unready(tableId, true);
    };
    return ResultAsync.fromSafePromise(setUp());
  });

  function unready(tableId: string, hasNameColumn: boolean) {
    void queryClient.invalidateQueries({ queryKey });
    return {
      tableId,
      ready: false as const,
      message: hasNameColumn
        ? 'Your table is ready, but could not be loaded. Try again to open it.'
        : 'Your table was created, but its Name column could not be added. Retry setup or open the table to continue.',
    };
  }
}
