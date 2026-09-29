import { createFreshSearch, type TimestampedItem } from '@core/util/freshSort';
import type { EntityItem, QuickAccessItem } from './types';

type Rankable = {
  searchText: string;
  timestamps: TimestampedItem;
  bucket?: string;
};

const createQuickAccessSearch = <T extends Rankable>() =>
  createFreshSearch<T>({
    config: { useViewedAt: true },
    getName: (item) => item.searchText,
    isChannelItem: (item) => item.bucket === 'channel',
    getTimestamp: (item) => item.timestamps,
  });

const quickAccessSearch = createQuickAccessSearch<QuickAccessItem>();
const rankableSearch = createQuickAccessSearch<Rankable>();

/** Ranks items quick access doesn't index with the same semantics as its own. */
export function searchLikeQuickAccess<T extends Rankable>(
  items: T[],
  query: string
): T[] {
  if (!query.trim()) return items;
  return rankableSearch(items, query).map(({ item }) => item as T);
}

/** Fuzzy-ranks entity candidates using the existing mentions semantics. */
export function searchQuickAccessItems(
  items: QuickAccessItem[],
  query: string
): QuickAccessItem[] {
  if (!query.trim()) return items;
  return quickAccessSearch(items, query).map(({ item }) => item);
}

/** Fuzzy-ranks entity candidates using the existing mentions semantics. */
export function searchQuickAccessEntities(
  items: EntityItem[],
  query: string
): EntityItem[] {
  return searchQuickAccessItems(items, query) as EntityItem[];
}
