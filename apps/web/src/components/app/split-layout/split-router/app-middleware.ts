import { getPreferredCalendarPeriodView } from '@app/features/calendar/calendar-preferences';
import {
  CALENDAR_ROUTE_ID,
  CALENDAR_SEARCH_NAMESPACE,
  calendarPath,
} from '@app/features/calendar-view/calendar-url';
import { CALENDAR_VIEW_ID } from '@app/features/calendar-view/types';
import { channelsSearch } from '@app/features/channels-view/channels-route';
import { CRM_VIEW_URL_PARAM } from '@app/features/companies/crm/saved-views';
import {
  crmCompanyPath,
  crmContactPath,
  crmDetailSearch,
  crmViewSearch,
} from '@app/features/companies/crm-route';
import {
  driveDocumentFromContent,
  drivePath,
} from '@app/features/drive-view/primitives/drive-route';
import { driveDocumentBlockType } from '@app/features/drive-view/primitives/drive-route-schema';
import { driveSearch } from '@app/features/drive-view/primitives/drive-search';
import { URL_PARAMS as EMAIL_URL_PARAMS } from '@app/features/email-thread/core/location';
import { EMAIL_DETAIL_SEARCH_NAMESPACE } from '@app/features/email-view/email-route';
import {
  routeParams,
  type SerializedSearchParams,
  type SplitRouterMiddleware,
  type SplitRouterMiddlewareContext,
  type SplitRouterMiddlewareResult,
  type SplitSearchState,
} from '@app/lib/split-router';
import { replaceSplitSearchParams } from '@app/lib/split-router/search';
import { URL_PARAMS as CALL_URL_PARAMS } from '@block-call/constants';
import { URL_PARAMS as CHANNEL_URL_PARAMS } from '@block-channel/constants';
import { URL_PARAMS as MD_URL_PARAMS } from '@block-md/constants';
import { URL_PARAMS as PDF_URL_PARAMS } from '@block-pdf/constants';
import { COMMENT_LINK_PARAM } from '@core/messages/comment-link';
import { match } from 'ts-pattern';
import { appSplitRoutes } from './app-routes';
import { decodeLegacyPair } from './legacy-route';

type AppMiddlewareState = {
  isTouchDevice: () => boolean;
};

/** Upgrade legacy component and block URLs when supported; on touch, keep
 * document details in their full-block routes instead of inline Drive views. */
function redirectLegacyRoutes(
  { to, redirect }: SplitRouterMiddlewareContext,
  options: AppMiddlewareState
): SplitRouterMiddlewareResult {
  const route = to.location.route;

  const isTouch = options.isTouchDevice();

  if (route.matches[0].id === 'drive' && isTouch) {
    const { documentType, documentId } = routeParams(route);
    if (typeof documentType === 'string' && typeof documentId === 'string') {
      return redirect(
        `/${driveDocumentBlockType(documentType)}/${encodeURIComponent(documentId)}`
      );
    }
  }

  if (route.matches[0].id === 'call-detail') {
    const { callId } = routeParams(route);
    if (typeof callId === 'string') {
      return redirect(`/drive/call/${encodeURIComponent(callId)}`);
    }
  }
  if (route.matches[0].id === 'pr-detail') {
    const { foreignEntityId } = routeParams(route);
    if (typeof foreignEntityId === 'string') {
      return redirect(`/reviews/pr/${encodeURIComponent(foreignEntityId)}`);
    }
  }
  if (route.matches[0].id !== 'legacy-content') return;

  const { type, id } = routeParams(route);

  if (typeof type !== 'string' || typeof id !== 'string') return;

  const content = decodeLegacyPair(type, id);
  if (!content) return;

  const path = match(content)
    .with({ type: 'component', id: 'documents' }, () => '/drive')
    .with({ type: 'component', id: 'settings' }, () => '/settings')
    .with({ type: 'component', id: CALENDAR_VIEW_ID }, () =>
      calendarPath(getPreferredCalendarPeriodView())
    )
    .with({ type: 'component' }, ({ id }) =>
      appSplitRoutes.definitions.some((route) => route.id === `view-${id}`)
        ? `/${id}`
        : undefined
    )
    .with({ type: 'company' }, ({ id }) => crmCompanyPath(id))
    .with({ type: 'contact' }, ({ id }) => crmContactPath(id))
    .when(
      () => isTouch,
      () => undefined
    )
    .with({ type: 'email' }, ({ id }) => `/mail/${encodeURIComponent(id)}`)
    .with(
      { type: 'channel' },
      ({ id }) => `/channels/${encodeURIComponent(id)}`
    )
    .with({ type: 'task' }, ({ id }) => `/tasks/${encodeURIComponent(id)}`)
    .otherwise((content) => {
      const document = driveDocumentFromContent(content);
      return document
        ? drivePath({ kind: 'tab', tab: 'owned' }, document)
        : undefined;
    });

  if (!path) return;

  return redirect(path);
}

type LegacySearchMapping = {
  namespace: string;
  fields: ReadonlyArray<readonly [string, string]>;
};

function crmLegacySearchMappings(
  rootId: string | undefined,
  leafId: string | undefined
): LegacySearchMapping[] {
  const mappings: LegacySearchMapping[] = [];
  if (rootId === 'view-companies') {
    mappings.push({
      namespace: crmViewSearch.namespace,
      fields: [[CRM_VIEW_URL_PARAM, 'view']],
    });
  }
  if (
    leafId === 'companies-company' ||
    leafId === 'companies-company-contact' ||
    leafId === 'companies-contact'
  ) {
    mappings.push({
      namespace: crmDetailSearch.namespace,
      fields: [[COMMENT_LINK_PARAM, 'commentId']],
    });
  }
  return mappings;
}

function mergeLegacySearch(
  currentSearch: SplitSearchState | undefined,
  raw: URLSearchParams,
  mappings: LegacySearchMapping[]
): SplitSearchState | undefined {
  const search = { ...currentSearch };
  let changed = false;
  for (const { namespace, fields } of mappings) {
    const current = search[namespace] ?? {};
    const additions: SerializedSearchParams = {};

    for (const [legacyKey, field] of fields) {
      // Explicit canonical values, including empty ones, take precedence.
      if (Object.hasOwn(current, field)) continue;
      const values = raw.getAll(legacyKey);
      if (values.length) additions[field] = values;
    }

    if (!Object.keys(additions).length) continue;
    changed = true;
    search[namespace] = { ...current, ...additions };
  }
  return changed ? search : undefined;
}

/** On external entry, copy legacy detail query keys into the destination pane.
 * Keep repeated values and let explicit pane-local values take precedence. */
function migrateLegacySearch({
  to,
  path,
  cause,
  externalSearch,
  redirect,
}: SplitRouterMiddlewareContext): SplitRouterMiddlewareResult {
  if (cause !== 'initial' && cause !== 'external') return;

  if (!externalSearch) return;

  const rootId = to.location.route.matches[0]?.id;
  const leafId = to.location.route.matches.at(-1)?.id;
  const homeChannel =
    leafId === 'home-channel' ||
    (leafId === 'home-preview' &&
      routeParams(to.location.route).blockType === 'channel');
  const homeDocumentType =
    leafId === 'home-document'
      ? routeParams(to.location.route).documentType
      : leafId === 'home-preview'
        ? routeParams(to.location.route).blockType
        : undefined;
  const commentKey = (() => {
    switch (homeDocumentType) {
      case 'md':
      case 'task':
      case 'skill':
      case 'snippet':
      case 'spreadsheet':
        return MD_URL_PARAMS.commentId;
      case 'pdf':
        return PDF_URL_PARAMS.annotationId;
    }
  })();
  const homeDocumentMapping = commentKey
    ? {
        namespace: driveSearch.namespace,
        fields: [[commentKey, 'commentId']] as const,
      }
    : undefined;

  const detailMapping = match(leafId)
    .with('mail-thread', () => ({
      namespace: EMAIL_DETAIL_SEARCH_NAMESPACE,
      fields: [[EMAIL_URL_PARAMS.messageId, 'messageId']] as const,
    }))
    .when(
      (id) => id === 'channels-channel' || homeChannel,
      () => ({
        namespace: channelsSearch.namespace,
        fields: [
          [CHANNEL_URL_PARAMS.message, 'messageId'],
          [CHANNEL_URL_PARAMS.thread, 'threadId'],
        ] as const,
      })
    )
    .when(
      (id) => id === 'home-document' || id === 'home-preview',
      () => homeDocumentMapping
    )
    .with(CALENDAR_ROUTE_ID, () => ({
      namespace: CALENDAR_SEARCH_NAMESPACE,
      fields: [['eventId', 'eventId']] as const,
    }))
    .with('call-detail', 'drive-call', () => ({
      namespace: 'call-detail',
      fields: [[CALL_URL_PARAMS.transcriptId, 'transcriptId']] as const,
    }))
    .otherwise(() => undefined);

  const raw = new URLSearchParams(externalSearch);
  const mappings = crmLegacySearchMappings(rootId, leafId);
  if (detailMapping) mappings.push(detailMapping);
  if (!mappings.length) return;

  const search = mergeLegacySearch(to.location.search, raw, mappings);
  if (!search) return;

  const query = new URLSearchParams();

  // Middleware redirects describe one entry; the router assigns its pane index.
  replaceSplitSearchParams(query, [{ location: { search } }]);

  return redirect(`${path}?${query}`);
}

export function createAppSplitRouterMiddleware(
  state: AppMiddlewareState
): readonly SplitRouterMiddleware[] {
  return [
    (context) => redirectLegacyRoutes(context, state),
    migrateLegacySearch,
  ];
}
