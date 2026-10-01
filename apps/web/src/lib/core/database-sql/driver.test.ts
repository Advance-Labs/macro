import { errAsync, okAsync } from 'neverthrow';
import { describe, expect, it, vi } from 'vitest';
import { runDatabaseSql, runDatabaseSqlStatement } from './driver';
import type {
  Bin,
  DatabaseOp,
  GqlQuery,
  OpResult,
  Page,
} from './generated/types';
import { readTranscript, replay } from './tests/transcript';

describe('runDatabaseSql', () => {
  it('asks the source for each page the engine wants, following the cursor', async () => {
    const paging = readTranscript('paging');
    const page = vi.fn(
      (
        _query: GqlQuery,
        _needs: string[],
        cursor: string | null,
        _limit: number
      ) => {
        const exchange = paging.exchanges[cursor === null ? 0 : 1];
        if (!('page' in exchange)) throw new Error('recorded bins');
        return okAsync<Page>(exchange.page);
      }
    );

    const outcome = await runDatabaseSql(paging.catalog, paging.sql, {
      source: { page, bins: vi.fn() },
      open: replay(paging),
    });

    expect(outcome._unsafeUnwrap()).toEqual(paging.outcome);
    expect(page.mock.calls).toEqual([
      [
        {
          type: 'soup',
          table: '01990000-0000-7000-8000-00000000d001',
          propf: null,
          keyHint: null,
        },
        ['01990000-0000-7000-8000-00000000c001'],
        null,
        500,
      ],
      [
        {
          type: 'soup',
          table: '01990000-0000-7000-8000-00000000d001',
          propf: null,
          keyHint: null,
        },
        ['01990000-0000-7000-8000-00000000c001'],
        'second-page',
        500,
      ],
    ]);
  });

  it('answers a grouped count from the bins alone', async () => {
    const counts = readTranscript('count-per-option');
    const bins = vi.fn(() => {
      const exchange = counts.exchanges[0];
      if (!('bins' in exchange)) throw new Error('recorded a page');
      return okAsync<Bin[]>(exchange.bins);
    });
    const page = vi.fn();

    const outcome = await runDatabaseSql(counts.catalog, counts.sql, {
      source: { page, bins },
      open: replay(counts),
    });

    expect(outcome._unsafeUnwrap()).toEqual(counts.outcome);
    expect(bins.mock.calls).toEqual([
      [
        {
          type: 'groupSoup',
          table: '01990000-0000-7000-8000-00000000d001',
          propf: null,
          groupBy: '01990000-0000-7000-8000-00000000c003',
        },
      ],
    ]);
    expect(page).not.toHaveBeenCalled();
  });

  it('reports a statement the engine refuses as an engine failure, freeing nothing it never opened', async () => {
    const outcome = await runDatabaseSql(
      { tables: [] },
      'SELECT name FROM crm.deals',
      {
        source: { page: vi.fn(), bins: vi.fn() },
        open: async () => {
          throw 'no such table: crm.deals';
        },
      }
    );

    expect(outcome._unsafeUnwrapErr()).toEqual({
      kind: 'engine',
      message: 'no such table: crm.deals',
    });
  });

  it('frees the engine when the source fails', async () => {
    const paging = readTranscript('paging');
    const free = vi.fn();
    const open = replay(paging);

    const outcome = await runDatabaseSql(paging.catalog, paging.sql, {
      source: {
        page: () =>
          errAsync({ kind: 'fetch' as const, message: 'gateway timed out' }),
        bins: vi.fn(),
      },
      open: async (catalog, sql) => ({ ...(await open(catalog, sql)), free }),
    });

    expect(outcome._unsafeUnwrapErr()).toEqual({
      kind: 'fetch',
      message: 'gateway timed out',
    });
    expect(free).toHaveBeenCalledTimes(1);
  });

  it('reads the rows an UPDATE matches, then sends its one op to the sink', async () => {
    const update = readTranscript('update-uniform');
    const [read, write] = update.exchanges;
    if (!('page' in read) || !('results' in write) || write.step.step !== 'ops')
      throw new Error('recorded a read, then a write');
    const recorded = write.step;
    const apply = vi.fn((_database: string, _ops: DatabaseOp[]) =>
      okAsync<OpResult[]>(write.results)
    );

    const outcome = await runDatabaseSqlStatement(update.catalog, update.sql, {
      source: { page: () => okAsync(read.page), bins: vi.fn() },
      ops: { apply },
      open: replay(update),
    });

    expect(outcome._unsafeUnwrap()).toEqual(update.outcome);
    expect(outcome._unsafeUnwrap().changesApplied).toBe(2);
    expect(apply.mock.calls).toEqual([[recorded.database, recorded.ops]]);
  });

  it('refuses a write where only reads run, sending nothing', async () => {
    const insert = readTranscript('insert-two-rows');

    const outcome = await runDatabaseSql(insert.catalog, insert.sql, {
      source: { page: vi.fn(), bins: vi.fn() },
      open: replay(insert),
    });

    expect(outcome._unsafeUnwrapErr()).toEqual({ kind: 'read-only' });
  });

  it('reports the sink refusing a write as an ops failure carrying the refusal', async () => {
    const update = readTranscript('update-uniform');
    const [read] = update.exchanges;
    if (!('page' in read)) throw new Error('recorded a read first');
    const refusal = {
      code: 'INVALID_OP' as const,
      message: 'op 0, row 1: "Done" is not an option of "Status"',
      refusal: { op: 0, row: 1, column: null },
    };

    const outcome = await runDatabaseSqlStatement(update.catalog, update.sql, {
      source: { page: () => okAsync(read.page), bins: vi.fn() },
      ops: { apply: () => errAsync(refusal) },
      open: replay(update),
    });

    expect(outcome._unsafeUnwrapErr()).toEqual({ kind: 'ops', error: refusal });
  });
});
