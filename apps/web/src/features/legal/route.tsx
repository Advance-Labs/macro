import { defineRoute } from '@app/lib/split-router';
import {
  usePageViewTracking,
  withAuth,
} from '@components/app/split-layout/split-router/app-route-shell';
import { lazy } from 'solid-js';

const Legal = lazy(async () => ({ default: (await import('./legal')).Legal }));
export const LegalRouteView = withAuth(() => {
  usePageViewTracking('legal');
  return <Legal />;
});
export const legalRoute = defineRoute({
  id: 'view-legal',
  path: 'legal',
  component: LegalRouteView,
  search: '*' as const,
  claim: () => ({ namespace: 'component', id: 'legal' }),
});
