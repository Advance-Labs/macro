import {
  doneRemindersFilter,
  firedRemindersFilter,
  scheduledRemindersFilter,
} from '@app/features/next-soup/filters/predicates';
import {
  clause,
  compileClause,
  confine,
  type FacetSelection,
  NIL_UUID,
  type SoupSearchRequest,
} from '@app/features/soup';
import type { EntityData } from '@entity';
import type { SoupAstItemsQueryArgs } from '@queries/soup/items';
import type { SearchSoupQueryArgs } from '@queries/soup/search';
import type { EntityFilters } from '@service-search/generated/models';
import { match } from 'ts-pattern';
import type { EmailFilterGroupId, ReminderStatusFilter } from '../types';

export const REMINDER_STATUS_GROUP_ID: EmailFilterGroupId = 'reminders';

/**
 * The status the Reminders tab lists when nothing is selected. Active is the
 * inbox — fired and waiting on you — the same default the standalone
 * Reminders view had before it moved under Email.
 */
export const DEFAULT_REMINDER_STATUS: ReminderStatusFilter = 'active';

const REMINDER_STATUSES: ReminderStatusFilter[] = [
  'active',
  'scheduled',
  'done',
];

const isReminderStatus = (value: string): value is ReminderStatusFilter =>
  (REMINDER_STATUSES as string[]).includes(value);

/** The single-select status the facets carry, or the default when unset. */
export function reminderStatusFromFacets(
  facets: FacetSelection
): ReminderStatusFilter {
  const selected = facets[REMINDER_STATUS_GROUP_ID]?.[0];
  return selected !== undefined && isReminderStatus(selected)
    ? selected
    : DEFAULT_REMINDER_STATUS;
}

/**
 * The server-side split. `reminderCompleted: false` on the open statuses is
 * load-bearing beyond filtering: it is what the normalized cache matches (as
 * `"comp":false`) to drop a reminder the moment it is marked done, rather
 * than on the next refetch. `reminderFired` is resolved against the database
 * clock — a client timestamp would land in the query key and change every
 * render.
 */
function statusClause(status: ReminderStatusFilter) {
  return match(status)
    .with('active', () =>
      clause.and(
        clause.eq('includeReminders', true),
        clause.eq('reminderCompleted', false),
        clause.eq('reminderFired', true)
      )
    )
    .with('scheduled', () =>
      clause.and(
        clause.eq('includeReminders', true),
        clause.eq('reminderCompleted', false),
        clause.eq('reminderFired', false)
      )
    )
    .with('done', () =>
      clause.and(
        clause.eq('includeReminders', true),
        clause.eq('reminderCompleted', true)
      )
    )
    .exhaustive();
}

/**
 * Builds the reminders-only Soup query for the Email view's Reminders tab.
 * Reminders are the one entity type that is opt-in server-side, so naming
 * `includeReminders` both surfaces them and — via `confine`, which
 * NIL-excludes every target the query does not reference — keeps every other
 * type out. Soup orders reminders by when they fire.
 */
export function buildReminderQuery(
  status: ReminderStatusFilter
): SoupAstItemsQueryArgs {
  return {
    params: {
      expand: true,
      limit: 100,
      sort_method: 'updated_at',
      // Active and Done are archives of what already happened, so newest
      // first like every other feed. Scheduled points at future dates, where
      // newest-first would put December above tomorrow.
      sort_direction: status === 'scheduled' ? 'asc' : 'desc',
    },
    body: compileClause(confine({ remf: statusClause(status) })),
  };
}

/** Item-level mirror of `statusClause`, for rows the cache hands back. */
export function reminderMatchesStatus(
  entity: EntityData,
  status: ReminderStatusFilter
): boolean {
  return match(status)
    .with('active', () => firedRemindersFilter(entity))
    .with('scheduled', () => scheduledRemindersFilter(entity))
    .with('done', () => doneRemindersFilter(entity))
    .exhaustive();
}

// Every type the search service knows about other than reminders matches
// nothing, so a search from the Reminders tab returns reminders alone.
const nonReminderFilters: EntityFilters = {
  calendar_event_filters: { calendar_event_ids: [NIL_UUID] },
  call_filters: { call_ids: [NIL_UUID] },
  channel_filters: { channel_ids: [NIL_UUID] },
  channel_thread_filters: { thread_ids: [NIL_UUID] },
  chat_filters: { chat_ids: [NIL_UUID] },
  crm_company_filters: { company_ids: [NIL_UUID] },
  document_filters: { document_ids: [NIL_UUID] },
  email_filters: { email_thread_ids: [NIL_UUID] },
  foreign_entity_filters: { ids: [NIL_UUID] },
  project_filters: { project_ids: [NIL_UUID] },
};

/** Mirrors the Reminders tab's status scoping for service-backed search. */
export function buildReminderSearchRequest(
  status: ReminderStatusFilter,
  search: SoupSearchRequest
): SearchSoupQueryArgs {
  const reminderFilters: NonNullable<EntityFilters['reminder_filters']> = match(
    status
  )
    .with('active', () => ({
      include: true,
      completed: false,
      fired: true,
    }))
    .with('scheduled', () => ({
      include: true,
      completed: false,
      fired: false,
    }))
    .with('done', () => ({ include: true, completed: true }))
    .exhaustive();

  return {
    params: { cursor: null, page_size: 100 },
    body: {
      query: search.query,
      match_type: search.matchType,
      search_on: 'name_content',
      filters: { ...nonReminderFilters, reminder_filters: reminderFilters },
    },
  };
}
