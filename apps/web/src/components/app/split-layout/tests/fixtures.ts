import {
  createMemoryHistory,
  createMemoryPaneStore,
  createSplitRouter,
  type Entry,
  type PaneId,
  type SplitLocation,
  type SplitRouterMiddleware,
  type SplitRoutes,
  type SplitSearchUpdate,
} from '@app/lib/split-router';
import { paneRoute } from '@app/routes/app-route';
import {
  agentChatsRoute,
  appRoute,
  channelDetailRoute,
  channelsSplitRoute,
  driveFolderDocumentRoute,
  driveFolderRoute,
  driveRootDocumentRoute,
  driveSplitRoute,
  driveTabDocumentRoute,
  driveTabRoute,
  emailSplitRoute,
  emailThreadRoute,
  homeChannelRoute,
  homeDocumentRoute,
  homePreviewRoute,
  homeSplitRoute,
  legacyContentRoute,
  notFoundRoute,
  searchRoute,
  taskDetailRoute,
  tasksSplitRoute,
} from '@app/routes/routes';
import type { BlockOrchestrator } from '@core/orchestrator';
import { createSplitLayout, type SplitManager } from '../layoutManager';
import { createAppPanePolicy } from '../split-router/app-pane-policy';
import {
  resolveContentLocation,
  splitContentFromLocation,
} from '../split-router/legacy-route';

const homeLocation: SplitLocation = {
  route: paneRoute({ id: 'view-home', params: {} }),
};

const routes: SplitRoutes = {
  definitions: [
    {
      ...appRoute,
      children: [
        homeSplitRoute,
        { ...channelsSplitRoute, children: [channelDetailRoute] },
        legacyContentRoute,
        notFoundRoute,
      ],
    },
  ],
  defaultRoute: () => homeLocation.route,
};

export const detailRoutes: SplitRoutes = {
  definitions: [
    {
      ...appRoute,
      children: [
        {
          ...homeSplitRoute,
          children: [homeDocumentRoute, homeChannelRoute, homePreviewRoute],
        },
        { ...channelsSplitRoute, children: [channelDetailRoute] },
        {
          ...driveSplitRoute,
          children: [
            driveRootDocumentRoute,
            { ...driveFolderRoute, children: [driveFolderDocumentRoute] },
            { ...driveTabRoute, children: [driveTabDocumentRoute] },
          ],
        },
        { ...emailSplitRoute, children: [emailThreadRoute] },
        agentChatsRoute,
        { ...tasksSplitRoute, children: [taskDetailRoute] },
        searchRoute,
        legacyContentRoute,
        notFoundRoute,
      ],
    },
  ],
  defaultRoute: () => homeLocation.route,
};

/**
 * A split manager over a router with a few of the app's pane routes, loaded
 * from `url` (panes joined by `/~/`). Call it inside a reactive root.
 */
export function createRoutedSplitLayout(
  orchestrator: BlockOrchestrator,
  url: string
): SplitManager {
  return createRoutedDetailLayout(orchestrator, url, routes).manager;
}

export function createRoutedDetailLayout(
  orchestrator: BlockOrchestrator,
  url: string,
  routes: SplitRoutes = detailRoutes,
  middleware: readonly SplitRouterMiddleware[] = []
) {
  let manager: SplitManager | undefined;
  const router = createSplitRouter({
    routes,
    middleware,
    history: createMemoryHistory(url),
    paneStore: createMemoryPaneStore<Entry>(),
    policy: createAppPanePolicy({
      manager: () => manager,
      toContent: splitContentFromLocation,
      defaultLocation: () => homeLocation,
      stacked: () => false,
    }),
  });
  manager = createSplitLayout(orchestrator, {
    router,
    toLocation: (content) => resolveContentLocation(router.routes, content),
    toContent: splitContentFromLocation,
  });
  return { manager, router };
}

type Router = ReturnType<typeof createSplitRouter>;

export function locationFor(router: Router, id: string) {
  return router.entry(id as PaneId)?.location;
}

export function routeFor(router: Router, id: string) {
  return locationFor(router, id)?.route;
}

export function searchFor(router: Router, id: string, namespace: string) {
  return locationFor(router, id)?.search?.[namespace];
}

export function historyFor(router: Router, id: string) {
  return router.history(id as PaneId);
}

export function updateSearchFor(
  router: Router,
  id: string,
  namespace: string,
  update: SplitSearchUpdate
) {
  return router.updateSearch(id as PaneId, namespace, update);
}
