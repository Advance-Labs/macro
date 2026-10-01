import {
  type FetchWithTokenInit,
  fetchWithToken,
} from '@core/util/fetchWithToken';
import { err, ok } from 'neverthrow';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getDatabaseQuery } from './database-queries';
import { databasesClient } from './databases';

vi.mock('@core/util/fetchWithToken', () => ({ fetchWithToken: vi.fn() }));
const fetch = vi.mocked(fetchWithToken);

/** The service answers `status` with `body`; the client's own handler words it. */
function answer(status: number, body: string) {
  fetch.mockImplementation(
    async (_input: RequestInfo, init?: FetchWithTokenInit<string>) => {
      const handler = init?.errorResponseHandler;
      if (!handler) throw new Error('the client must word its own failures');
      return err([await handler(new Response(body, { status }))]);
    }
  );
}

beforeEach(() => {
  fetch.mockReset();
});

describe('databases client failures', () => {
  it('names a refused schema change INVALID_SCHEMA with the service’s message', async () => {
    answer(400, JSON.stringify({ message: 'A column named Status exists.' }));

    const renamed = await databasesClient.renameColumn({
      id: 'db',
      tableId: 'table',
      columnId: 'column',
      name: 'Status',
      previousName: 'State',
    });

    expect(renamed._unsafeUnwrapErr()).toEqual([
      { code: 'INVALID_SCHEMA', message: 'A column named Status exists.' },
    ]);
  });

  it('keeps the transport’s codes for missing, forbidden and conflicting requests', async () => {
    answer(404, JSON.stringify({ message: 'not found' }));
    expect(
      (await databasesClient.get({ id: 'db' }))._unsafeUnwrapErr()
    ).toEqual([{ code: 'NOT_FOUND', message: 'not found' }]);

    answer(403, JSON.stringify({ message: 'unauthorized' }));
    expect(
      (await databasesClient.get({ id: 'db' }))._unsafeUnwrapErr()
    ).toEqual([{ code: 'FORBIDDEN', message: 'unauthorized' }]);

    answer(
      409,
      JSON.stringify({
        message: 'The table changed. Refresh before entering this value.',
      })
    );
    expect(
      (
        await databasesClient.reorderColumns({
          id: 'db',
          tableId: 'table',
          columnIds: ['a', 'b'],
          baseVersion: 3,
        })
      )._unsafeUnwrapErr()
    ).toEqual([
      {
        code: 'CONFLICT',
        message: 'The table changed. Refresh before entering this value.',
      },
    ]);

    answer(502, 'Bad gateway');
    expect((await databasesClient.list())._unsafeUnwrapErr()).toEqual([
      { code: 'SERVER_ERROR', message: 'Bad gateway' },
    ]);
  });

  it('reads a 400 on a read route as an HTTP error, not a refusal', async () => {
    answer(400, '');

    expect(
      (
        await databasesClient.columnCasts({
          id: 'db',
          tableId: 'table',
          columnId: 'column',
        })
      )._unsafeUnwrapErr()
    ).toEqual([{ code: 'HTTP_ERROR', message: 'HTTP error! status: 400' }]);
  });

  it('carries the op, row and column an /ops refusal names', async () => {
    answer(
      400,
      JSON.stringify({
        message: 'op 0, row 1, column col-status: "Done" is not an option',
        op: 0,
        row: 1,
        column: 'col-status',
      })
    );

    const applied = await databasesClient.applyOps({
      id: 'db',
      request: { ops: [] },
    });

    expect(applied._unsafeUnwrapErr()).toEqual([
      {
        code: 'INVALID_OP',
        message: 'op 0, row 1, column col-status: "Done" is not an option',
        refusal: {
          message: 'op 0, row 1, column col-status: "Done" is not an option',
          op: 0,
          row: 1,
          column: 'col-status',
        },
      },
    ]);
  });

  it('leaves the refusal empty when the body is not an op refusal', async () => {
    answer(400, JSON.stringify({ message: 'op 0: the table has no rows' }));

    const applied = await databasesClient.applyOps({
      id: 'db',
      request: { ops: [] },
    });

    expect(applied._unsafeUnwrapErr()).toEqual([
      {
        code: 'INVALID_OP',
        message: 'op 0: the table has no rows',
        refusal: null,
      },
    ]);
  });

  it('passes a success through untouched', async () => {
    fetch.mockResolvedValue(ok({ results: [] }));

    const applied = await databasesClient.applyOps({
      id: 'db',
      request: { ops: [] },
    });

    expect(applied._unsafeUnwrap()).toEqual({ results: [] });
  });
});

describe('saved query failures', () => {
  it('names each refusal by what the route says it means', async () => {
    answer(404, JSON.stringify({ message: 'not found' }));
    expect((await getDatabaseQuery('query'))._unsafeUnwrapErr()).toEqual([
      { code: 'NOT_FOUND', message: 'not found' },
    ]);

    answer(400, JSON.stringify({ message: 'unknown column "Stage"' }));
    expect((await getDatabaseQuery('query'))._unsafeUnwrapErr()).toEqual([
      { code: 'INVALID_QUERY', message: 'unknown column "Stage"' },
    ]);

    answer(403, JSON.stringify({ message: 'only SELECT' }));
    expect((await getDatabaseQuery('query'))._unsafeUnwrapErr()).toEqual([
      { code: 'READ_ONLY', message: 'only SELECT' },
    ]);

    answer(422, JSON.stringify({ message: 'budget' }));
    expect((await getDatabaseQuery('query'))._unsafeUnwrapErr()).toEqual([
      { code: 'BUDGET_EXCEEDED', message: 'budget' },
    ]);
  });
});
