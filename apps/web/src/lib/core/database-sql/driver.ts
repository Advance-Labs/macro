/**
 * Drives one `SELECT` through the staged engine: the engine says what to
 * fetch, the server fetches it, the driver feeds it back, until the engine
 * has the answer. The driver knows nothing about GraphQL; a `RequestServer`
 * does (Soup for database tables, the users query for `people`).
 */

import { match } from 'ts-pattern';
import type { Bin, Catalog, Outcome, Page, Request, Step } from './protocol';
import type { Query } from './wasm-module';

/** Answers the engine's requests. */
export interface RequestServer {
  /** One page of a `soup` or `people` request. */
  page: (request: Request) => Promise<Page>;
  /** The bins of a `groupSoup` request. */
  bins: (request: Request) => Promise<Bin[]>;
}

/** Opens engines; the wasm module in production, a fake in tests. */
export type QueryFactory = (catalog: Catalog, sql: string) => Query;

/** Run `sql` to its outcome, freeing the engine when done or on failure. */
export async function runQuery(
  open: QueryFactory,
  catalog: Catalog,
  sql: string,
  server: RequestServer
): Promise<Outcome> {
  const query = open(catalog, sql);
  try {
    let step = query.start();
    for (;;) {
      const next: Step | Outcome = await match(step)
        .with({ step: 'done' }, (done) => done)
        .with({ step: 'fetch' }, async (request) =>
          query.feed_page(request.id, await server.page(request))
        )
        .with({ step: 'bins' }, async (request) =>
          query.feed_bins(request.id, await server.bins(request))
        )
        .exhaustive();
      if (!('step' in next) || next.step === 'done') {
        const { step: _step, ...outcome } = next as Step & { step: 'done' };
        return outcome;
      }
      step = next;
    }
  } finally {
    query.free();
  }
}
