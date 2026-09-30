import { describe, expect, it, vi } from 'vitest';
import { DatabaseSqlError, runDatabaseSql } from './driver';
import type { Bin, GqlQuery, Page } from './protocol';
import { readTranscript, replay } from './tests/transcript';

describe('runDatabaseSql', () => {
  it('asks the source for each page the engine wants, following the cursor', async () => {
    const paging = readTranscript('paging');
    const page = vi.fn(
      async (
        _query: GqlQuery,
        _needs: string[],
        cursor: string | null,
        _limit: number
      ): Promise<Page> => {
        const exchange = paging.exchanges[cursor === null ? 0 : 1];
        if (!('page' in exchange)) throw new Error('recorded bins');
        return exchange.page;
      }
    );

    const outcome = await runDatabaseSql(paging.catalog, paging.sql, {
      source: { page, bins: vi.fn() },
      open: replay(paging),
    });

    expect(outcome).toEqual(paging.outcome);
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
    const bins = vi.fn(async (): Promise<Bin[]> => {
      const exchange = counts.exchanges[0];
      if (!('bins' in exchange)) throw new Error('recorded a page');
      return exchange.bins;
    });
    const page = vi.fn();

    const outcome = await runDatabaseSql(counts.catalog, counts.sql, {
      source: { page, bins },
      open: replay(counts),
    });

    expect(outcome).toEqual(counts.outcome);
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

  it('reports a statement the engine refuses as an error, freeing nothing it never opened', async () => {
    await expect(
      runDatabaseSql({ tables: [] }, 'SELECT name FROM crm.deals', {
        source: { page: vi.fn(), bins: vi.fn() },
        open: async () => {
          throw 'no such table: crm.deals';
        },
      })
    ).rejects.toEqual(new DatabaseSqlError('no such table: crm.deals'));
  });

  it('frees the engine when the source fails', async () => {
    const paging = readTranscript('paging');
    const free = vi.fn();
    const open = replay(paging);

    await expect(
      runDatabaseSql(paging.catalog, paging.sql, {
        source: {
          page: async () => {
            throw new Error('gateway timed out');
          },
          bins: vi.fn(),
        },
        open: async (catalog, sql) => ({ ...(await open(catalog, sql)), free }),
      })
    ).rejects.toThrow('gateway timed out');
    expect(free).toHaveBeenCalledTimes(1);
  });
});
