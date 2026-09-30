import {
  buildFlatSoupRows,
  createSearchState,
  createSoupLoadMoreRow,
} from '@app/features/soup';
import { withEntityNotifications } from '@app/features/soup/entity-notifications';
import { useGlobalNotificationSource } from '@components/app/GlobalAppState';
import type { EntityData } from '@entity';
import { useSoupAstItemsQuery } from '@queries/soup/items';
import { createMemo } from 'solid-js';
import {
  buildReminderQuery,
  buildReminderSearchRequest,
  reminderMatchesStatus,
  reminderStatusFromFacets,
} from './reminder-query';
import type {
  EmailDataSource,
  EmailDataSourceInput,
  EmailDataSourceItem,
} from './use-email-query';

/**
 * The Reminders tab's list: the user's reminders in the selected status, from
 * Soup, with service search over the same slice. Inbox scope does not apply —
 * reminders belong to the user, not to a mailbox.
 */
export function useReminderSource(
  state: EmailDataSourceInput
): EmailDataSource {
  const notificationSource = useGlobalNotificationSource();
  const enabled = () => state.tab === 'reminders';
  const status = createMemo(() => reminderStatusFromFacets(state.facets));
  const queryArgs = createMemo(() => buildReminderQuery(status()));

  const query = useSoupAstItemsQuery(queryArgs, () => ({
    enabled: enabled(),
    // Websocket-driven cache inserts prepend into every matching query; only
    // reminders belong here. Status is re-checked on the rows below.
    meta: { insertFilter: (item) => item.tag === 'reminder' },
  }));

  // The quick-access pool behind local search holds no reminders, so every
  // search result comes from the search service.
  const search = createSearchState({
    text: () => state.search,
    enabled,
    disableLocalSearch: () => true,
    buildRequest: (request) => buildReminderSearchRequest(status(), request),
  });
  const usesServiceSearch = search.usesServiceSearch;

  const isListPending = () => query.isLoading || query.isPlaceholderData;

  const rawEntities = (): EntityData[] => {
    if (!search.isSearching()) {
      // Previous-status rows are not valid results for the new query.
      if (isListPending()) return [];
      return query.data?.entities ?? [];
    }
    if (!usesServiceSearch() || search.searchQuery.isPlaceholderData) return [];
    return search.data();
  };

  // A row the cache hands back may have moved status since the page was
  // fetched (marked done, come due); the page is the server's answer, the
  // status check keeps optimistic updates honest until the refetch lands.
  const entities = createMemo(() =>
    rawEntities()
      .filter((entity) => reminderMatchesStatus(entity, status()))
      .map((entity) => withEntityNotifications(entity, notificationSource))
  );

  const hasMore = () => {
    if (usesServiceSearch()) return search.hasNextPage();
    return !isListPending() && query.hasNextPage;
  };
  const isLoadingMore = () =>
    usesServiceSearch()
      ? search.isFetchingNextPage()
      : query.isFetchingNextPage;

  // Flat, like the standalone view was: Soup already orders reminders by
  // when they fire, and date headers would only restate that.
  const items = createMemo((): EmailDataSourceItem[] => {
    const rows: EmailDataSourceItem[] = buildFlatSoupRows(entities());
    if (hasMore()) {
      rows.push(
        createSoupLoadMoreRow({
          scopeId: `email:reminders:${status()}`,
          isLoading: isLoadingMore(),
        })
      );
    }
    return rows;
  });

  return {
    items,
    isLoading: () => {
      if (!search.isSearching()) return isListPending();
      if (entities().length > 0) return false;
      return usesServiceSearch() ? search.isLoading() : query.isLoading;
    },
    isFetching: () => {
      if (search.isSettling()) return true;
      return usesServiceSearch() ? search.isFetching() : query.isFetching;
    },
    error: () =>
      (usesServiceSearch() ? search.error() : query.error) ?? undefined,
    hasMore,
    isLoadingMore,
    loadMore: async () => {
      if (usesServiceSearch()) {
        await search.fetchNextPage();
        return;
      }
      await query.fetchNextPage();
    },
    refresh: async () => {
      if (usesServiceSearch()) {
        await search.refetch();
        return;
      }
      await query.refresh();
    },
  };
}
