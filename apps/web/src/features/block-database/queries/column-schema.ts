import { queryClient } from '@queries/client';
import { databasesKeys } from '@queries/storage/keys';
import { storageServiceClient } from '@service-storage/client';
import { ResultAsync } from 'neverthrow';
import { match } from 'ts-pattern';
import type {
  DatabaseColumnTypeChange,
  DatabaseSchemaChange,
} from '../core/column-schema';

type ColumnMutation =
  | { kind: 'type'; columnId: string; change: DatabaseColumnTypeChange }
  | { kind: 'delete'; columnId: string }
  | { kind: 'order'; columnIds: string[] };

/** Refresh schema and rows even on a conflict; never replay a destructive mutation. */
export function updateDatabaseColumns(params: {
  databaseId: string;
  tableId: string;
  baseVersion: number;
  mutation: ColumnMutation;
}): DatabaseSchemaChange {
  const common = {
    id: params.databaseId,
    tableId: params.tableId,
    baseVersion: params.baseVersion,
  };
  const applied = match(params.mutation)
    .with({ kind: 'type' }, ({ columnId, change }) =>
      storageServiceClient.databases.changeColumnType({
        ...common,
        columnId,
        request: { ...change, baseVersion: params.baseVersion },
      })
    )
    .with({ kind: 'delete' }, ({ columnId }) =>
      storageServiceClient.databases.deleteColumn({ ...common, columnId })
    )
    .with({ kind: 'order' }, ({ columnIds }) =>
      storageServiceClient.databases.reorderColumns({ ...common, columnIds })
    )
    .exhaustive();
  // Open reads rerun against the refreshed schema, whatever the outcome.
  const refreshed = async () => {
    const result = await applied;
    await queryClient.invalidateQueries(
      { queryKey: databasesKeys.detail(params.databaseId).queryKey },
      { throwOnError: false }
    );
    return result.map(() => undefined);
  };
  return new ResultAsync(refreshed());
}
