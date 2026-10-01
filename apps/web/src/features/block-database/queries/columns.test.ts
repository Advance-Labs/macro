import type { NewOption } from '@core/database-sql/generated/types';
import { okAsync } from 'neverthrow';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { addDatabaseColumnOptions, createDatabaseColumn } from './columns';

const storage = vi.hoisted(() => ({
  applyDatabaseOps: vi.fn(),
  invalidateDatabase: vi.fn(),
}));
vi.mock('@queries/storage/databases', () => storage);

const uuidv7 =
  /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

afterEach(() => {
  vi.resetAllMocks();
});

describe('adding columns and options', () => {
  it('creates a select column with its options, each under a minted id, in one create_column op', async () => {
    storage.applyDatabaseOps.mockReturnValue(
      okAsync([{ kind: 'column_created', column: 'status', tableVersion: 3 }])
    );

    const created = await createDatabaseColumn({
      databaseId: 'db',
      tableId: 'tasks',
      name: 'Status',
      type: { type: 'select', multi: false },
      options: ['Todo', 'Done'],
    });

    expect(storage.applyDatabaseOps).toHaveBeenCalledExactlyOnceWith('db', [
      {
        kind: 'create_column',
        table: 'tasks',
        id: expect.stringMatching(uuidv7),
        definition: {
          source: 'new',
          name: 'Status',
          type: { type: 'select', multi: false },
          options: [
            { id: expect.stringMatching(uuidv7), label: 'Todo' },
            { id: expect.stringMatching(uuidv7), label: 'Done' },
          ],
        },
      },
    ]);
    const [[, [op]]] = storage.applyDatabaseOps.mock.calls;
    const ids = [
      op.id,
      ...op.definition.options.map((option: NewOption) => option.id),
    ];
    expect(new Set(ids).size).toBe(3);
    expect(created._unsafeUnwrap()).toBe(op.id);
    expect(storage.invalidateDatabase).toHaveBeenCalledExactlyOnceWith('db');
  });

  it('adds options to a column under minted ids and reads the schema again', async () => {
    storage.applyDatabaseOps.mockReturnValue(
      okAsync([{ kind: 'options_added', tableVersion: 4, added: ['done'] }])
    );

    const added = await addDatabaseColumnOptions({
      databaseId: 'db',
      tableId: 'tasks',
      columnId: 'status',
      labels: ['Done'],
    });

    expect(added.isOk()).toBe(true);
    expect(storage.applyDatabaseOps).toHaveBeenCalledExactlyOnceWith('db', [
      {
        kind: 'add_options',
        table: 'tasks',
        column: 'status',
        options: [{ id: expect.stringMatching(uuidv7), label: 'Done' }],
      },
    ]);
    expect(storage.invalidateDatabase).toHaveBeenCalledExactlyOnceWith('db');
  });
});
