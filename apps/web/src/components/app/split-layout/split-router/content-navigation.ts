import type { SplitRouter, SplitRoutesManifest } from '@app/lib/split-router';
import { encodeRoute, getRouteClaim } from '@app/lib/split-router/routes';
import { replaceSplitSearchParams } from '@app/lib/split-router/search';
import type {
  OpenWithSplitOptions,
  SplitContent,
  SplitId,
  SplitManager,
} from '../layoutManager';
import { openAppSplitLocation } from '../splitRouterLayout';
import { resolveContentLocation } from './legacy-route';

export function createContentNavigator(
  manager: SplitManager,
  router: SplitRouter<SplitId>,
  routes: SplitRoutesManifest
) {
  return (content: SplitContent, options: OpenWithSplitOptions) => {
    const firstVisible = manager.getVisibleSplits()[0];
    const source =
      options.handle ??
      manager.activeSplit() ??
      (firstVisible ? manager.getSplit(firstVisible.id) : undefined);
    if (!source) return;
    let location = resolveContentLocation(routes, content);
    const current = router.location(source.id);
    const requestedClaim = getRouteClaim(routes, location.route);
    const currentClaim = current && getRouteClaim(routes, current.route);
    // A location request for the current owner must retain its surrounding route
    // and search, rather than redirecting through the content's legacy URL.
    if (
      options.search &&
      current &&
      requestedClaim &&
      currentClaim &&
      requestedClaim.namespace === currentClaim.namespace &&
      requestedClaim.id === currentClaim.id
    ) {
      location = current;
    }
    const path = `/${encodeRoute(routes, { location }).map(encodeURIComponent).join('/')}`;
    const query = new URLSearchParams();
    replaceSplitSearchParams(query, [{ location }]);
    let onApplied = options.onApplied;
    const reportApplied = () => {
      const callback = onApplied;
      onApplied = undefined;
      callback?.();
    };
    router.navigate(source.id, query.size ? `${path}?${query}` : path, {
      target:
        options.preferNewSplit && manager.canAppendSplit()
          ? 'new-split'
          : 'current',
      replace: options.mergeHistory,
      search: options.search,
      onApplied: reportApplied,
      open: (request) =>
        openAppSplitLocation(manager, routes, request, content, {
          ...options,
          onApplied: reportApplied,
        }),
    });
  };
}
