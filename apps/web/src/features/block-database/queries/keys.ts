import { createQueryKeys } from '@lukemorales/query-key-factory';

export const databaseViewKeys = createQueryKeys('database-views', {
  saved: null,
});

export const databaseColumnKeys = createQueryKeys('database-columns', {
  /** A column's type-change dry run, as of one table version. */
  casts: (
    databaseId: string,
    tableId: string,
    columnId: string,
    version: number
  ) => ({
    queryKey: [databaseId, tableId, columnId, version],
  }),
});
