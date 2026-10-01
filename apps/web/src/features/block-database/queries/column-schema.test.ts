import { okAsync } from 'neverthrow';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { updateDatabaseColumns } from './column-schema';

const storage = vi.hoisted(() => ({ applyDatabaseOps: vi.fn() }));
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
  vi.resetAllMocks();
});

describe('column schema changes', () => {
  it('sends a type change with clearInvalid against the table version it was made at', async () => {
    storage.applyDatabaseOps.mockReturnValue(
      okAsync([
        {
          kind: 'column_typed',
          tableVersion: 5,
          clearedCells: 3,
          trimmedCells: 0,
        },
      ])
    );

    const changed = await updateDatabaseColumns({
      databaseId: 'db',
      tableId: 'tasks',
      baseVersion: 4,
      mutation: {
        kind: 'type',
        columnId: 'price',
        change: { to: { type: 'number' }, clearInvalid: true, baseVersion: 4 },
      },
    });

    expect(changed.isOk()).toBe(true);
    expect(storage.applyDatabaseOps).toHaveBeenCalledExactlyOnceWith(
      'db',
      [
        {
          kind: 'change_column_type',
          table: 'tasks',
          column: 'price',
          to: { type: 'number' },
          clearInvalid: true,
        },
      ],
      { tasks: 4 }
    );
  });

  it('names this database as a relation target', async () => {
    storage.applyDatabaseOps.mockReturnValue(
      okAsync([
        {
          kind: 'column_typed',
          tableVersion: 5,
          clearedCells: 0,
          trimmedCells: 0,
        },
      ])
    );

    await updateDatabaseColumns({
      databaseId: 'db',
      tableId: 'tasks',
      baseVersion: 4,
      mutation: {
        kind: 'type',
        columnId: 'owner',
        change: { to: { type: 'relation', table: 'people' } },
      },
    });

    expect(storage.applyDatabaseOps).toHaveBeenCalledExactlyOnceWith(
      'db',
      [
        {
          kind: 'change_column_type',
          table: 'tasks',
          column: 'owner',
          to: { type: 'relation', database: 'db', table: 'people' },
          clearInvalid: false,
        },
      ],
      { tasks: 4 }
    );
  });

  it('deletes a column against the table version', async () => {
    storage.applyDatabaseOps.mockReturnValue(
      okAsync([{ kind: 'column_deleted', tableVersion: 8 }])
    );

    await updateDatabaseColumns({
      databaseId: 'db',
      tableId: 'tasks',
      baseVersion: 7,
      mutation: { kind: 'delete', columnId: 'notes' },
    });

    expect(storage.applyDatabaseOps).toHaveBeenCalledExactlyOnceWith(
      'db',
      [{ kind: 'delete_column', table: 'tasks', column: 'notes' }],
      { tasks: 7 }
    );
  });

  it('reorders columns against the table version', async () => {
    storage.applyDatabaseOps.mockReturnValue(
      okAsync([{ kind: 'columns_reordered', tableVersion: 9 }])
    );

    await updateDatabaseColumns({
      databaseId: 'db',
      tableId: 'tasks',
      baseVersion: 8,
      mutation: { kind: 'order', columnIds: ['title', 'status'] },
    });

    expect(storage.applyDatabaseOps).toHaveBeenCalledExactlyOnceWith(
      'db',
      [{ kind: 'reorder_columns', table: 'tasks', order: ['title', 'status'] }],
      { tasks: 8 }
    );
  });
});
