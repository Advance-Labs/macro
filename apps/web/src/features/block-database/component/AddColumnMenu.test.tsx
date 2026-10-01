import type { ResultError } from '@core/util/result';
import type { DatabaseSchemaErrorCode } from '@service-storage/databases';
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@solidjs/testing-library';
import { err, okAsync, type Result, ResultAsync } from 'neverthrow';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { AddColumnMenu } from './AddColumnMenu';

const createColumn = vi.hoisted(() => vi.fn());
vi.mock('@queries/storage/databases', () => ({
  createDatabaseColumn: createColumn,
}));
vi.mock('@core/mobile/isMobile', () => ({ isMobile: () => false }));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('immediate column creation', () => {
  it('creates an inferred Text column without a popup and focuses its header', async () => {
    createColumn.mockReturnValue(okAsync('column'));
    const created = vi.fn(() => true);
    render(() => (
      <AddColumnMenu
        databaseId="db"
        tableId="table"
        columns={[]}
        onCreated={created}
      />
    ));
    fireEvent.click(screen.getByRole('button', { name: 'Add column' }));
    await waitFor(() =>
      expect(created).toHaveBeenCalledExactlyOnceWith('column')
    );
    expect(createColumn).toHaveBeenCalledExactlyOnceWith({
      databaseId: 'db',
      tableId: 'table',
      request: {
        inferType: true,
        binding: {
          kind: 'new',
          name: 'Unnamed',
          dataType: 'STRING',
          isMultiSelect: false,
        },
      },
    });
    expect(screen.queryByRole('dialog')).toBeNull();
  });
  it('prevents repeated clicks during creation and preserves errors', async () => {
    let settle!: (
      result: Result<string, ResultError<DatabaseSchemaErrorCode>[]>
    ) => void;
    createColumn.mockImplementation(
      () =>
        new ResultAsync(
          new Promise<Result<string, ResultError<DatabaseSchemaErrorCode>[]>>(
            (resolve) => {
              settle = resolve;
            }
          )
        )
    );
    render(() => (
      <AddColumnMenu databaseId="db" tableId="table" columns={[]} />
    ));
    const add = screen.getByRole('button', { name: 'Add column' });
    fireEvent.click(add);
    fireEvent.click(add);
    expect(createColumn).toHaveBeenCalledTimes(1);
    settle(err([{ code: 'NETWORK_ERROR', message: 'Connection lost' }]));
    expect((await screen.findByRole('alert')).textContent).toBe(
      'Your change could not be sent. Check your connection.'
    );
  });
});
