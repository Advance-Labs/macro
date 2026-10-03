import { blockMetadata } from '@app/lib/constants/block-metadata';
import {
  defineBlock,
  type ExtractLoadType,
  LoadErrors,
  loadResult,
} from '@core/block';
import { Model } from '@core/component/AI/constant/model';
import { fetchAndCacheChat } from '@queries/cognition/chat-data';
import type { Entity } from '@service-cognition/generated/schemas/entity';
import type { DocumentMetadata } from '@service-storage/generated/schemas/documentMetadata';
import { err, ok } from 'neverthrow';
import BlockChat from './component/Block';

export { DEFAULT_CHAT_NAME } from '@app/lib/constants/block-metadata';

export type AttachmentWithoutId = Entity;

export const definition = defineBlock({
  ...blockMetadata.chat,
  name: 'chat',
  component: BlockChat,
  liveTrackingEnabled: true,
  async load(source, intent) {
    if (source.type === 'dss') {
      // Fetch the chat from dcs
      const chatId = source.id;
      const res = await loadResult(fetchAndCacheChat(chatId));
      if (res.isErr()) return err(res.error);
      const chat = res.value;

      if (intent === 'preload') {
        return ok({
          type: 'preload',
          origin: source,
        });
      }

      return ok({
        ...chat,
        allModels: Object.values(Model),
        documentMetadata: {
          documentId: chat.chat.id,
          documentName: chat.chat.name,
          documentVersionId: 1,
          owner: chat.chat.userId,
          createdAt: chat.chat.createdAt,
          updatedAt: chat.chat.updatedAt,
          deletedAt: null,
          fileType: 'chat' as any,
        } satisfies DocumentMetadata,
      });
    }

    return LoadErrors.MISSING;
  },
});

export type ChatData = ExtractLoadType<(typeof definition)['load']>;
