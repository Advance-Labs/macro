import { canvasLocationUpdates } from '@app/features/block-canvas/canvas-route';
import { URL_PARAMS as CHANNEL_URL_PARAMS } from '@app/features/block-channel/constants';
import { chatLocationUpdates } from '@app/features/block-chat/chat-route';
import { URL_PARAMS as MARKDOWN_URL_PARAMS } from '@app/features/block-md/constants';
import { markdownLocationUpdates } from '@app/features/block-md/markdown-route';
import { pdfLocationUpdates } from '@app/features/block-pdf/pdf-route';
import { channelLocationUpdates } from '@app/features/channels-view/channels-route';
import { URL_PARAMS as EMAIL_URL_PARAMS } from '@app/features/email-thread/core/location';
import { emailLocationUpdates } from '@app/features/email-view/email-route';
import type { BlockAlias, BlockName } from '@app/lib/constants/block-registry';
import { fileTypeToBlockName } from '@app/lib/constants/file-metadata';
import { useSplitLayout } from '@components/app/split-layout/layout';
import { useSplitNavigationHandler } from '@core/util/useSplitNavigationHandler';
import { createCallback } from '@solid-primitives/rootless';
import type { ParentProps } from 'solid-js';
import { match } from 'ts-pattern';

export function documentLocationUpdates(
  blockName: BlockName | BlockAlias,
  id: string,
  params: Record<string, string>
) {
  if (blockName !== 'channel' && Object.keys(params).length === 0) return;
  return match(blockName)
    .with('md', 'task', 'snippet', 'skill', () => {
      const nodeId = params[MARKDOWN_URL_PARAMS.nodeId];
      const commentId = params[MARKDOWN_URL_PARAMS.commentId];
      if (!nodeId && !commentId) return;
      return markdownLocationUpdates(id, { nodeId, commentId });
    })
    .with('channel', () => {
      const messageId = params[CHANNEL_URL_PARAMS.message];
      return channelLocationUpdates(
        messageId
          ? {
              kind: 'message',
              messageId,
              threadId: params[CHANNEL_URL_PARAMS.thread],
            }
          : { kind: 'latest' }
      );
    })
    .with('pdf', () => pdfLocationUpdates(id, params))
    .with('canvas', () => canvasLocationUpdates(id, params))
    .with('chat', () => chatLocationUpdates(id, params))
    .with('email', () => {
      const messageId = params[EMAIL_URL_PARAMS.messageId];
      if (messageId) return emailLocationUpdates(messageId);
    })
    .otherwise(() => undefined);
}

export function openDocument(
  blockOrFileType: string,
  id: string,
  params?: Record<string, string>,
  inNewSplit?: boolean,
  onApplied?: VoidFunction
) {
  const { openWithSplit } = useSplitLayout();

  const targetBlock = fileTypeToBlockName(blockOrFileType);
  if (!targetBlock) return;

  const search = documentLocationUpdates(targetBlock, id, params ?? {});
  return openWithSplit(
    { type: targetBlock, id, ...(!search && { params }) },
    {
      preferNewSplit: inNewSplit,
      search,
      onApplied,
    }
  );
}

export function BlockLink(
  props: ParentProps<{
    blockOrFileName: string;
    id: string;
    params?: Record<string, string>;
  }>
) {
  const open = createCallback((e: MouseEvent) => {
    let newSplit = e.shiftKey;
    openDocument(props.blockOrFileName, props.id, props.params, newSplit);
  });
  const navHandlers = useSplitNavigationHandler<HTMLSpanElement>(open);
  return <span {...navHandlers}>{props.children}</span>;
}
