import { queryClient } from '@queries/client';
import { databasesKeys } from '@queries/storage/keys';
import type { DatabaseDetail } from '@service-storage/generated/schemas/databaseDetail';
import { errAsync, okAsync } from 'neverthrow';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { reorderDatabaseViews } from './views';

const transport = vi.hoisted(() => ({
  applyDatabaseOps: vi.fn(),
  applyDatabaseTableVersions: vi.fn(),
  invalidateDatabase: vi.fn(),
}));
vi.mock('@queries/storage/databases', () => transport);
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
      views: [
        {
          id: 'first',
          databaseId: 'db',
          tableId: 'invites',
          name: 'First',
          position: 'a0',
          query: { filter: null, sort: [] },
          layout: { kind: 'table', columns: [] },
          createdAt: '2026-10-01T00:00:00Z',
          updatedAt: '2026-10-01T00:00:00Z',
        },
        {
          id: 'second',
          databaseId: 'db',
          tableId: 'invites',
          name: 'Second',
          position: 'a1',
          query: { filter: null, sort: [] },
          layout: { kind: 'table', columns: [] },
          createdAt: '2026-10-01T00:00:00Z',
          updatedAt: '2026-10-01T00:00:00Z',
        },
      ],
    },
  ],
};

function cachedViews() {
  return queryClient
    .getQueryData<DatabaseDetail>(databasesKeys.detail('db').queryKey)
    ?.tables[0].views.map((view) => [view.id, view.position]);
}

describe('reordering a table’s views', () => {
  it('sends reorder_views with every view, shows the order at once and keeps the keys the server wrote', async () => {
    queryClient.setQueryData(databasesKeys.detail('db').queryKey, detail);
    transport.applyDatabaseOps.mockReturnValue(
      okAsync([
        {
          kind: 'views_reordered',
          tableVersion: 4,
          positions: [
            { view: 'second', position: 'Zz' },
            { view: 'first', position: 'a0' },
          ],
        },
      ])
    );

    const reordered = reorderDatabaseViews('db', 'invites', [
      'second',
      'first',
    ]);
    expect(cachedViews()).toEqual([
      ['second', 'a1'],
      ['first', 'a0'],
    ]);
    expect((await reordered).isOk()).toBe(true);

    expect(transport.applyDatabaseOps).toHaveBeenCalledExactlyOnceWith('db', [
      { kind: 'reorder_views', table: 'invites', order: ['second', 'first'] },
    ]);
    expect(transport.applyDatabaseTableVersions).toHaveBeenCalledWith('db', {
      invites: 4,
    });
    expect(cachedViews()).toEqual([
      ['second', 'Zz'],
      ['first', 'a0'],
    ]);
  });

  it('reads the views again when the server refuses the order', async () => {
    queryClient.setQueryData(databasesKeys.detail('db').queryKey, detail);
    transport.applyDatabaseOps.mockReturnValue(
      errAsync({
        code: 'INVALID_OP',
        message: 'the order must name every view of the table once',
        refusal: {
          op: 0,
          row: null,
          column: null,
          message: 'the order must name every view of the table once',
        },
      })
    );

    const reordered = await reorderDatabaseViews('db', 'invites', ['second']);

    expect(reordered.isErr()).toBe(true);
    expect(transport.invalidateDatabase).toHaveBeenCalledExactlyOnceWith('db');
  });
});
