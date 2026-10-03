import { defineRoute } from '@app/lib/split-router';
import {
  usePageViewTracking,
  withAuth,
} from '@components/app/split-layout/split-router/app-route-shell';
import { lazy } from 'solid-js';

const Support = lazy(async () => ({
  default: (await import('./Support')).Support,
}));
export const SupportRouteView = withAuth(
  (props: {
    initialTicket?: string;
    companyId?: string;
    contactId?: string;
  }) => {
    usePageViewTracking('support');
    const params = new URLSearchParams(window.location.search);
    return (
      <Support
        initialTicket={props.initialTicket ?? params.get('ticket') ?? undefined}
        companyId={props.companyId ?? params.get('companyId') ?? undefined}
        contactId={props.contactId ?? params.get('contactId') ?? undefined}
      />
    );
  }
);
export const supportRoute = defineRoute({
  id: 'view-support',
  path: 'support',
  component: SupportRouteView,
  search: '*' as const,
  externalSearch: ['ticket', 'companyId', 'contactId'],
  claim: () => ({ namespace: 'component', id: 'support' }),
});
