import {
  createSearchParamsCodec,
  type SplitSearchUpdate,
} from '@app/lib/split-router';
import { z } from 'zod';
import type { EmailTab } from './types';

export const emailTabSearch = {
  namespace: 'mail',
  schema: z.object({
    tab: z.enum([
      'important',
      'noise',
      'favorites',
      'sent',
      'scheduled',
      'reminders',
      'calendar',
      'drafts',
      'shared',
      'all',
    ]),
  }),
  defaults: { tab: 'important' as EmailTab },
};

export const emailTabSearchCodec = createSearchParamsCodec(emailTabSearch);

export const EMAIL_DETAIL_SEARCH_NAMESPACE = 'email-detail';

export const emailDetailSearch = {
  namespace: EMAIL_DETAIL_SEARCH_NAMESPACE,
  schema: z.object({ messageId: z.string(), seek: z.string() }),
  defaults: { messageId: '', seek: '' },
};

export const emailDetailSearchCodec =
  createSearchParamsCodec(emailDetailSearch);

/** Replace the message target without changing the owning mail view. */
export function emailLocationUpdates(
  messageId: string,
  seek = crypto.randomUUID()
): Record<string, SplitSearchUpdate> {
  const target = emailDetailSearchCodec.serialize({ messageId, seek });
  return {
    [emailDetailSearch.namespace]: (current) => {
      const { messageId: _messageId, seek: _seek, ...rest } = current ?? {};
      return { ...rest, ...target };
    },
  };
}
