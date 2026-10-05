import {
  createSearchParamsCodec,
  type SplitSearchUpdate,
} from '@app/lib/split-router';
import type { ChannelTargetRequest } from '@channel/Channel/ChannelSurface';
import { z } from 'zod';
import type { ChannelsQueryScope, ChannelsTab } from './types';

export const channelsSearch = {
  namespace: 'channels',
  schema: z.object({
    tab: z.enum(['browse', 'recents', 'threads']),
    mobileTab: z.enum(['channels', 'direct_messages', 'recents']),
    messageId: z.string(),
    threadId: z.string(),
    latest: z.boolean(),
    seek: z.string(),
  }),
  defaults: {
    tab: 'browse' as ChannelsTab,
    mobileTab: 'channels' as ChannelsQueryScope,
    messageId: '',
    threadId: '',
    latest: false,
    seek: '',
  },
};

export const channelsSearchCodec = createSearchParamsCodec(channelsSearch);

/** Replace a channel target, preserving the owner's tab and list filters. */
export function channelLocationUpdates(
  location: ChannelTargetRequest,
  seek = crypto.randomUUID()
): Record<string, SplitSearchUpdate> {
  const target = channelsSearchCodec.serialize({
    ...channelsSearch.defaults,
    messageId: location.kind === 'message' ? location.messageId : '',
    threadId: location.kind === 'message' ? (location.threadId ?? '') : '',
    latest: location.kind === 'latest',
    seek,
  });
  return {
    [channelsSearch.namespace]: (current) => {
      const {
        messageId: _messageId,
        threadId: _threadId,
        latest: _latest,
        seek: _seek,
        ...rest
      } = current ?? {};
      return { ...rest, ...target };
    },
  };
}
