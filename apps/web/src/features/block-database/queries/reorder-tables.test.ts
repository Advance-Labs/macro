import { queryClient } from '@queries/client';
import { databasesKeys } from '@queries/storage/keys';
import type { DatabaseDetail } from '@service-storage/generated/schemas/databaseDetail';
import type { Table } from '@service-storage/generated/schemas/table';
import type { ResultError } from '@core/util/result';
import { err, type Result, ResultAsync } from 'neverthrow';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { reorderDatabaseTables } from './reorder-tables';

const transport = vi.hoisted(() => ({ reorderTables: vi.fn() }));
vi.mock('@service-storage/client', () => ({
  storageServiceClient: { databases: transport },
}));
const storage = vi.hoisted(() => ({ invalidateDatabase: vi.fn() }));
vi.mock('@queries/storage/databases', () => storage);
vi.mock('@queries/client', async () => {
  const { QueryClient } = await import('@tanstack/solid-query');
  return {
    queryClient: new QueryClient({
      defaultOptions: { queries: { retry: false } },
    }),
  };
});

afterEach(() => {
  queryClient.clear();
  vi.resetAllMocks();
});

const detail: DatabaseDetail = {
  database: {
    id: 'db',
    name: 'Party Planner',
    owner_id: 'owner',
    created_at: '',
    trashed_at: null,
  },
  grant: 'owner',
  tables: [
    {
      table: {
        id: 'invites',
        database_id: 'db',
        name: 'Invites',
        position: 'a',
        version: 3,
      },
      sql_name: '"Invites"',
      read_sql_name: '"Invites"',
      columns: [],
      views: [],
    },
    {
      table: {
        id: 'venues',
        database_id: 'db',
        name: 'Venues',
        position: 'b',
        version: 1,
      },
      sql_name: '"Venues"',
      read_sql_name: '"Venues"',
      columns: [],
      views: [],
    },
  ],
};

describe('reordering tables', () => {
  it('puts back only the tab order when the move is refused, keeping a rename made meanwhile', async () => {
    const key = databasesKeys.detail('db').queryKey;
    queryClient.setQueryData(key, detail);
    const { promise: answered, resolve: answer } =
      Promise.withResolvers<Result<Table[], ResultError<string>[]>>();
    transport.reorderTables.mockReturnValue(new ResultAsync(answered));

    const reordered = reorderDatabaseTables({
      databaseId: 'db',
      tableIds: ['venues', 'invites'],
    });
    await vi.waitFor(() => expect(transport.reorderTables).toHaveBeenCalled());
    queryClient.setQueryData(key, (current: DatabaseDetail | undefined) =>
      current
        ? {
            ...current,
            tables: current.tables.map((entry) =>
              entry.table.id === 'invites'
                ? { ...entry, table: { ...entry.table, name: 'Guests' } }
                : entry
            ),
          }
        : current
    );
    answer(
      err([
        { name: 'Error', code: 'FORBIDDEN', message: 'Owner access required' },
      ])
    );

    expect((await reordered).isErr()).toBe(true);
    expect(
      queryClient
        .getQueryData<DatabaseDetail>(key)
        ?.tables.map((entry) => entry.table.name)
    ).toEqual(['Guests', 'Venues']);
    expect(storage.invalidateDatabase).toHaveBeenCalledExactlyOnceWith('db');
  });
});
