import { createDatabaseRelations } from '@app/features/block-database/queries/database-relations';
import { idToDisplayName } from '@core/user/util';
import { useDatabaseTableChanges } from '@queries/storage/databases-sync';
import { match } from 'ts-pattern';
import type { AnswerDisplay } from '../context/answer-display';

/**
 * People are named from the contacts the engine reads them from; related
 * rows by their table's title, from one live read per related table.
 */
export const answerNames: AnswerDisplay['names'] = (answer) => {
  const relations = createDatabaseRelations({
    targets: () => [
      ...new Map(
        (answer()?.columns ?? []).flatMap((column) => {
          const related = column.source?.relatedTable;
          return related ? [[related.tableId, related] as const] : [];
        })
      ).values(),
    ],
    onTableChanged: (listener) =>
      useDatabaseTableChanges((change) => listener(change.tableId)),
  });
  return () =>
    ({ kind, id, table }) =>
      match(kind)
        .with('USER', () => idToDisplayName(id) || undefined)
        .with('DATABASE_ROW', () =>
          table
            ? relations(table)
                .rows()
                .find((row) => row.id === id)?.name
            : undefined
        )
        .otherwise(() => undefined);
};
