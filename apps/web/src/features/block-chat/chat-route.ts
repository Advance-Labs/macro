import {
  createSearchParamsCodec,
  type SplitSearchUpdate,
} from '@app/lib/split-router';
import { z } from 'zod';

export const chatDetailSearch = {
  namespace: 'chat-detail',
  schema: z.object({
    chatId: z.string(),
    messageId: z.string(),
    seek: z.string(),
  }),
  defaults: { chatId: '', messageId: '', seek: '' },
};
export const chatDetailSearchCodec = createSearchParamsCodec(chatDetailSearch);

export function chatLocationUpdates(
  chatId: string,
  params: Record<string, string>,
  seek = crypto.randomUUID()
): Record<string, SplitSearchUpdate> {
  const target = chatDetailSearchCodec.serialize({
    chatId,
    messageId: params.message_id ?? '',
    seek,
  });
  return {
    [chatDetailSearch.namespace]: (current) => {
      const {
        chatId: _chatId,
        messageId: _messageId,
        seek: _seek,
        ...rest
      } = current ?? {};
      return { ...rest, ...target };
    },
  };
}
