import { describe, expect, it, vi } from 'vitest';
import { runQuery } from './driver';
import type { Catalog, Outcome, Page, Request, Step } from './protocol';
import type { Query } from './wasm-module';

const catalog: Catalog = { tables: [] };

const outcome: Outcome = {
  columns: [{ name: 'email', kind: 'text' }],
  rows: [[{ type: 'text', value: 'sam@example.com' }]],
  rowIds: [],
  readTables: [],
  truncated: false,
  insertedRowIds: [],
  changesApplied: 0,
  failures: [],
};

const request = (id: number, cursor: string | null): Request => ({
  id,
  query: { type: 'people', ids: null },
  needs: [],
  cursor,
  limit: 500,
});

/** An engine that wants two pages of people, then answers. */
function fakeQuery(): Query & { fed: [number, Page][]; freed: boolean } {
  const query = {
    fed: [] as [number, Page][],
    freed: false,
    start: (): Step => ({ step: 'fetch', ...request(0, null) }),
    feed_page: (id: number, page: Page): Step => {
      query.fed.push([id, page]);
      return page.next
        ? { step: 'fetch', ...request(id + 1, page.next) }
        : { step: 'done', ...outcome };
    },
    feed_bins: (): Step => {
      throw new Error('no bins');
    },
    free: () => {
      query.freed = true;
    },
  };
  return query;
}

describe('runQuery', () => {
  it('feeds every requested page back by id until the engine is done', async () => {
    const query = fakeQuery();
    const pages: Page[] = [
      { rows: [{ id: 'a', cells: {} }], next: 'more' },
      { rows: [{ id: 'b', cells: {} }], next: null },
    ];
    const server = {
      page: vi.fn(async (req: Request) => pages[req.cursor ? 1 : 0]),
      bins: vi.fn(),
    };

    const result = await runQuery(() => query, catalog, 'SELECT …', server);

    expect(result).toEqual(outcome);
    expect(server.page).toHaveBeenCalledTimes(2);
    expect(server.page.mock.calls[1][0].cursor).toBe('more');
    expect(query.fed.map(([id]) => id)).toEqual([0, 1]);
    expect(server.bins).not.toHaveBeenCalled();
    expect(query.freed).toBe(true);
  });

  it('frees the engine when the server fails', async () => {
    const query = fakeQuery();
    await expect(
      runQuery(() => query, catalog, 'SELECT …', {
        page: async () => {
          throw new Error('gateway timed out');
        },
        bins: async () => [],
      })
    ).rejects.toThrow('gateway timed out');
    expect(query.freed).toBe(true);
  });
});
