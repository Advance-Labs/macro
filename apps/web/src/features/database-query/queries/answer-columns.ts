import { toViewColumn } from '@app/features/block-database/queries/table-rows';
import { throwOnErr } from '@core/util/result';
import { databasesKeys } from '@queries/storage/keys';
import { storageServiceClient } from '@service-storage/client';
import type { DatabaseDetail } from '@service-storage/databases';
import { useQueries } from '@tanstack/solid-query';
import type {
  AnswerDisplay,
  ResultColumnLookup,
} from '../context/answer-display';
import type { ResultColumn } from '../core/answer-cell';

/** Finds the database column a result column was read from. */
export function resultColumnLookup(
  details: readonly DatabaseDetail[]
): ResultColumnLookup {
  return (column: ResultColumn) => {
    const origin = column.origin;
    if (!origin) return;
    const [tableName, columnName] = origin;
    for (const detail of details) {
      const table = detail.tables.find(
        (candidate) => candidate.table.name === tableName
      );
      const found = table?.columns.find(
        (candidate) =>
          (candidate.column.display_name ??
            candidate.definition.definition.display_name) === columnName
      );
      if (found) return toViewColumn(found);
    }
  };
}

/** Column types for the databases an answer read, from the schema cache. */
export const answerResultColumns: AnswerDisplay['columns'] = (answer) => {
  const details = useQueries(() => ({
    queries: [...new Set(answer()?.read_database_ids ?? [])].map((id) => ({
      queryKey: databasesKeys.detail(id).queryKey,
      queryFn: (): Promise<DatabaseDetail> =>
        throwOnErr(() => storageServiceClient.databases.get({ id })),
      staleTime: 30_000,
    })),
  }));
  return () =>
    resultColumnLookup(
      details.flatMap((detail) => (detail.isSuccess ? [detail.data] : []))
    );
};
