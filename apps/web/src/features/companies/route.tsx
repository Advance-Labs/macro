import { createSearchParams, defineRoute } from '@app/lib/split-router';
import { CRM_VIEW_URL_PARAM } from '@companies/crm/saved-views';
import {
  RedirectSplit,
  usePageViewTracking,
  withAuth,
} from '@components/app/split-layout/split-router/app-route-shell';
import { uuidRouteReference } from '@components/app/split-layout/split-router/mention-links';
import { enableCrm, isFeatureEnabled } from '@core/constant/featureFlags';
import { COMMENT_LINK_PARAM } from '@core/messages/comment-link';
import { lazy } from 'solid-js';
import { z } from 'zod';
import { getViewPreset } from '../next-soup/sidebar/soup-filter-presets';
import { crmDetailSearch, crmViewSearch } from './crm-route';

const SoupView = lazy(async () => ({
  default: (await import('../next-soup/soup-view/soup-view')).SoupView,
}));

export const CompaniesRouteView = withAuth(() => {
  if (!isFeatureEnabled(enableCrm))
    return <RedirectSplit to={{ type: 'component', id: 'home' }} />;
  usePageViewTracking('companies');
  const preset = getViewPreset('companies');
  const [search] = createSearchParams(crmViewSearch);
  return (
    <SoupView
      viewName="Customers"
      initialFilters={preset?.filters}
      initialClientFilters={preset?.clientFilters}
      initialGroupBy={preset?.groupBy}
      initialCrmView={search.view}
    />
  );
});

export const companyContactRoute = defineRoute({
  id: 'companies-company-contact',
  path: 'contact/:contactId',
  params: z.object({ contactId: z.string().min(1) }),
  search: [crmDetailSearch.namespace],
  externalSearch: [COMMENT_LINK_PARAM],
  remountKey: ({ contactId }) => contactId,
  claim: ({ contactId }) => ({
    namespace: 'block',
    id: `contact:${contactId}`,
  }),
  toReference: ({ contactId }) => uuidRouteReference(contactId, 'contact'),
});

export const companyRoute = defineRoute({
  id: 'companies-company',
  path: 'company/:companyId',
  params: z.object({ companyId: z.string().min(1) }),
  search: [crmDetailSearch.namespace],
  externalSearch: [COMMENT_LINK_PARAM],
  remountKey: ({ companyId }) => companyId,
  claim: ({ companyId }) => ({
    namespace: 'block',
    id: `company:${companyId}`,
  }),
  toReference: ({ companyId }) => uuidRouteReference(companyId, 'company'),
  children: [companyContactRoute],
});

export const contactRoute = defineRoute({
  id: 'companies-contact',
  path: 'contact/:contactId',
  params: z.object({ contactId: z.string().min(1) }),
  search: [crmDetailSearch.namespace],
  externalSearch: [COMMENT_LINK_PARAM],
  remountKey: ({ contactId }) => contactId,
  claim: ({ contactId }) => ({
    namespace: 'block',
    id: `contact:${contactId}`,
  }),
  toReference: ({ contactId }) => uuidRouteReference(contactId, 'contact'),
});

export const companiesRoute = defineRoute({
  id: 'view-companies',
  path: 'companies',
  component: CompaniesRouteView,
  search: [crmViewSearch.namespace],
  externalSearch: [CRM_VIEW_URL_PARAM],
  claim: () => ({ namespace: 'component', id: 'companies' }),
  children: [companyRoute, contactRoute],
});
