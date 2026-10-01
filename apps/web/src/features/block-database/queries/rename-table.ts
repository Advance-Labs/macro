import { queryClient } from '@queries/client';
import { invalidateDatabase } from '@queries/storage/databases';
import { databasesKeys } from '@queries/storage/keys';
import { storageServiceClient } from '@service-storage/client';
import type { DatabaseDetail } from '@service-storage/generated/schemas/databaseDetail';
import type { DatabaseSchemaChange } from '../core/column-schema';

export function renameDatabaseTable(params: {
  databaseId: string;
  tableId: string;
  name: string;
  previousName: string;
}): DatabaseSchemaChange {
  return storageServiceClient.databases
    .renameTable({
      id: params.databaseId,
      tableId: params.tableId,
      name: params.name,
      previousName: params.previousName,
    })
    .mapErr((errors) => {
      void invalidateDatabase(params.databaseId);
      return errors;
    })
    .map(async (renamed) => {
      // An older request must not overwrite the committed name after navigation.
      await queryClient.cancelQueries({ queryKey: databasesKeys.detail._def });
      queryClient.setQueryData(
        databasesKeys.detail(params.databaseId).queryKey,
        (previous: DatabaseDetail | undefined) =>
          previous && {
            ...previous,
            tables: previous.tables.map((entry) =>
              entry.table.id === params.tableId &&
              entry.table.version <= renamed.version
                ? { ...entry, table: renamed }
                : entry
            ),
          }
      );
      // SQL names can change throughout the catalog; open reads rerun against it.
      // A refresh failure does not turn the committed rename into a failed write.
      await queryClient.invalidateQueries(
        { queryKey: databasesKeys.detail._def },
        { throwOnError: false }
      );
    });
}
