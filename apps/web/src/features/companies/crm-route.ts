import {
  type CreateSearchParamsOptions,
  createSearchParamsCodec,
  takeLast,
} from '@app/lib/split-router';
import { COMMENT_LINK_PARAM } from '@core/messages/comment-link';
import { getWebOrigin } from '@core/util/webOrigin';
import { z } from 'zod';
import {
  type CrmViewConfig,
  decodeCrmViewParam,
  encodeCrmViewParam,
  isCrmViewConfig,
} from './crm/saved-views';

const crmViewSearchSchema = z.object({
  view: z
    .custom<CrmViewConfig | undefined>(
      (value) => value === undefined || isCrmViewConfig(value)
    )
    .optional(),
});

export type CrmViewSearchParams = z.infer<typeof crmViewSearchSchema>;

export const crmViewSearch = {
  namespace: 'crm',
  schema: crmViewSearchSchema,
  defaults: { view: undefined } as CrmViewSearchParams,
  serialize(value) {
    return value.view ? { view: [encodeCrmViewParam(value.view)] } : undefined;
  },
  deserialize(params) {
    const encoded = takeLast(params.view);
    return encoded ? { view: decodeCrmViewParam(encoded) } : {};
  },
} satisfies CreateSearchParamsOptions<CrmViewSearchParams>;

export const crmViewSearchCodec = createSearchParamsCodec(crmViewSearch);

export const crmDetailSearch = {
  namespace: 'crm-detail',
  schema: z.object({ commentId: z.string() }),
  defaults: { commentId: '' },
};

export const crmDetailSearchCodec = createSearchParamsCodec(crmDetailSearch);

export function crmCompanyPath(companyId: string): string {
  return `/companies/company/${encodeURIComponent(companyId)}`;
}

export function crmContactPath(contactId: string): string {
  return `/companies/contact/${encodeURIComponent(contactId)}`;
}

function buildCrmUrl(path: string, commentId?: string): string {
  const url = new URL(`/app${path}`, getWebOrigin());
  if (commentId) url.searchParams.set(COMMENT_LINK_PARAM, commentId);
  return url.toString();
}

export function buildCrmCompanyUrl(
  companyId: string,
  commentId?: string
): string {
  return buildCrmUrl(crmCompanyPath(companyId), commentId);
}

export function buildCrmContactUrl(
  contactId: string,
  commentId?: string
): string {
  return buildCrmUrl(crmContactPath(contactId), commentId);
}
