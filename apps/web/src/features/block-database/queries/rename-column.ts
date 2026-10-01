import { queryClient } from '@queries/client';
import { databasesKeys } from '@queries/storage/keys';
import { storageServiceClient } from '@service-storage/client';
import type { DatabaseDetail } from '@service-storage/generated/schemas/databaseDetail';
import type { DatabaseSchemaChange } from '../core/column-schema';

/** Change the placement label while preserving column IDs and stored SQL. */
export function renameDatabaseColumn(params: {
  databaseId: string;
  tableId: string;
  columnId: string;
  name: string;
  previousName: string;
}): DatabaseSchemaChange {
  const queryKey = databasesKeys.detail(params.databaseId).queryKey;
  return storageServiceClient.databases
    .renameColumn({
      id: params.databaseId,
      tableId: params.tableId,
      columnId: params.columnId,
      name: params.name,
      previousName: params.previousName,
    })
    .mapErr((errors) => {
      void queryClient.invalidateQueries({ queryKey });
      return errors;
    })
    .map(async (renamed) => {
      // Cancel an older schema read before seeding the committed label. A newer
      // version may contain another rename, so a delayed response must not replace it.
      await queryClient.cancelQueries({ queryKey });
      queryClient.setQueryData(
        queryKey,
        (previous: DatabaseDetail | undefined) =>
          previous && {
            ...previous,
            tables: previous.tables.map((entry) =>
              entry.table.id === params.tableId &&
              entry.table.version <= renamed.table_version
                ? {
                    ...entry,
                    table: { ...entry.table, version: renamed.table_version },
                    columns: entry.columns.map((column) =>
                      column.column.id === params.columnId
                        ? { ...column, column: renamed.column }
                        : column
                    ),
                  }
                : entry
            ),
          }
      );
      // Open reads rerun against the refreshed schema. The refresh may not report
      // an already-committed rename as a failed write.
      await queryClient.invalidateQueries(
        { queryKey },
        { throwOnError: false }
      );
    });
}
