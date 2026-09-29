import {
  createRoutesManifest,
  decodeRoute,
  encodeRoute,
  getExternalSearchKeys,
  getRouteClaim,
  getRouteSearchNamespaces,
  routeParams,
} from '@app/lib/split-router';
import { describe, expect, it, vi } from 'vitest';
import { buildCrmViewShareUrl } from './crm/saved-views';
import {
  buildCrmCompanyUrl,
  buildCrmContactUrl,
  crmDetailSearchCodec,
  crmViewSearchCodec,
} from './crm-route';
import { companiesRoute } from './route';

vi.mock('@components/app/split-layout/split-router/app-route-shell', () => ({
  withAuth: (value: unknown) => value,
  RedirectSplit: () => null,
  usePageViewTracking: () => {},
}));
vi.mock('@core/constant/featureFlags', () => ({
  enableCrm: { key: 'enable-crm' },
  isFeatureEnabled: () => true,
}));
vi.mock('../next-soup/sidebar/soup-filter-presets', () => ({
  getViewPreset: () => undefined,
}));

const companyId = '019507e8-14a3-7bc1-8610-419f16bd03a8';
const contactId = '019507e8-14a3-7bc1-8610-419f16bd03a9';
const manifest = createRoutesManifest({ definitions: [companiesRoute] });

describe('CRM routes', () => {
  it('routes and claims a company inside Customers', () => {
    const segments = ['companies', 'company', companyId];
    const entry = decodeRoute(manifest, segments)!;

    expect(encodeRoute(manifest, entry)).toEqual(segments);
    expect(routeParams(entry.location.route)).toEqual({ companyId });
    expect(getRouteClaim(manifest, entry.location.route)).toEqual({
      namespace: 'block',
      id: `company:${companyId}`,
    });
    expect(getRouteSearchNamespaces(manifest, entry.location.route)).toEqual(
      new Set(['crm', 'crm-detail'])
    );
  });

  it('retains company ancestry for a nested contact', () => {
    const segments = ['companies', 'company', companyId, 'contact', contactId];
    const entry = decodeRoute(manifest, segments)!;

    expect(encodeRoute(manifest, entry)).toEqual(segments);
    expect(routeParams(entry.location.route)).toEqual({
      companyId,
      contactId,
    });
    expect(getRouteClaim(manifest, entry.location.route)).toEqual({
      namespace: 'block',
      id: `contact:${contactId}`,
    });
  });

  it('supports direct contact links without a known company', () => {
    const segments = ['companies', 'contact', contactId];
    const entry = decodeRoute(manifest, segments)!;

    expect(encodeRoute(manifest, entry)).toEqual(segments);
    expect(routeParams(entry.location.route)).toEqual({ contactId });
    expect(getRouteClaim(manifest, entry.location.route)).toEqual({
      namespace: 'block',
      id: `contact:${contactId}`,
    });
    expect(getExternalSearchKeys(manifest, [entry])).toEqual(
      new Set(['crmView', 'comment_id'])
    );
  });
});

describe('CRM route search', () => {
  it('round-trips shared view state and drops malformed payloads', () => {
    const view = {
      kind: 'crm' as const,
      searchText: 'Acme',
      viewMode: 'list' as const,
    };
    const encoded = crmViewSearchCodec.serialize({ view });

    expect(crmViewSearchCodec.parse(encoded)).toEqual({
      valid: true,
      value: { view },
    });
    expect(crmViewSearchCodec.serialize({ view: undefined })).toBeUndefined();
    expect(crmViewSearchCodec.parse({ view: ['not-base64'] })).toEqual({
      valid: true,
      value: { view: undefined },
    });
    const share = new URL(buildCrmViewShareUrl(view));
    expect(share.pathname).toBe('/app/companies');
    expect(share.searchParams.get('crmView')).toBeTruthy();
  });

  it('round-trips discussion targets', () => {
    expect(
      crmDetailSearchCodec.parse(
        crmDetailSearchCodec.serialize({ commentId: 'message-1' })
      )
    ).toEqual({
      valid: true,
      value: { commentId: 'message-1' },
    });
  });

  it('builds canonical record links', () => {
    expect(new URL(buildCrmCompanyUrl(companyId)).pathname).toBe(
      `/app/companies/company/${companyId}`
    );
    const contact = new URL(buildCrmContactUrl(contactId, 'message-1'));
    expect(contact.pathname).toBe(`/app/companies/contact/${contactId}`);
    expect(contact.searchParams.get('comment_id')).toBe('message-1');
  });
});
