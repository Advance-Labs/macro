import {
  createSearchParams,
  createSearchParamsCodec,
  type SplitSearchUpdate,
  useOwnsSearchNamespace,
} from '@app/lib/split-router';
import type { Accessor } from 'solid-js';
import { z } from 'zod';
import type { DatabaseTarget } from './primitives/database-navigation';

export const databaseDetailSearch = {
  namespace: 'database-detail',
  schema: z.object({
    databaseId: z.string(),
    tableId: z.string(),
    viewId: z.string(),
    rowId: z.string(),
    seek: z.string(),
  }),
  defaults: { databaseId: '', tableId: '', viewId: '', rowId: '', seek: '' },
};

export const databaseDetailSearchCodec =
  createSearchParamsCodec(databaseDetailSearch);

/** Read only this route owner's target; local/popover hosts never borrow it. */
export function createDatabaseRouteTarget(
  enabled = true
): Accessor<DatabaseTarget> {
  const ownsSearch = useOwnsSearchNamespace(databaseDetailSearch.namespace);
  if (!enabled || !ownsSearch()) return () => databaseDetailSearch.defaults;
  const [search] = createSearchParams(databaseDetailSearch);
  return () => search;
}

/** Replace the target, not unrelated fields in the detail namespace. */
export function databaseLocationUpdates(
  databaseId: string,
  location: { tableId?: string; viewId?: string; rowId?: string },
  seek: string = crypto.randomUUID()
): Record<string, SplitSearchUpdate> {
  const target = databaseDetailSearchCodec.serialize({
    ...databaseDetailSearch.defaults,
    databaseId,
    ...location,
    seek,
  });
  return {
    [databaseDetailSearch.namespace]: (current) => {
      const {
        databaseId: _databaseId,
        tableId: _tableId,
        viewId: _viewId,
        rowId: _rowId,
        seek: _seek,
        ...rest
      } = current ?? {};
      return { ...rest, ...target };
    },
  };
}
