import { queryClient } from '@queries/client';
import { databasesKeys } from '@queries/storage/keys';
import { storageServiceClient } from '@service-storage/client';
import type { DatabaseSchemaChange } from '../core/column-schema';
import { patchTableColumn } from './detail-cache';

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
      // A newer version may contain another rename, so a delayed response
      // must not replace it.
      await patchTableColumn({
        databaseId: params.databaseId,
        tableId: params.tableId,
        columnId: params.columnId,
        tableVersion: renamed.table_version,
        change: (column) => ({ ...column, column: renamed.column }),
      });
      // Open reads rerun against the refreshed schema. The refresh may not report
      // an already-committed rename as a failed write.
      await queryClient.invalidateQueries(
        { queryKey },
        { throwOnError: false }
      );
    });
}
