import { markdownLocationUpdates } from '@app/features/block-md/markdown-route';
import { pdfLocationUpdates } from '@app/features/block-pdf/pdf-route';
import type { BlockAlias, BlockName } from '@app/lib/constants/block-registry';
import {
  type ItemLike,
  itemToBlockName,
  resolveBlockAlias,
} from '@app/lib/constants/file-metadata';
import type { SplitSearchUpdate } from '@app/lib/split-router';
import { URL_PARAMS as MD_URL_PARAMS } from '@block-md/constants';
import { URL_PARAMS as PDF_URL_PARAMS } from '@block-pdf/constants';
import { match } from 'ts-pattern';
import type { UnifiedNotification } from './types';

// Minimal entity shape — the live entity from the UI is authoritative when
// available (notification metadata is a snapshot at notification time and may
// lack `subType` for older events).
export type NotificationEntityOverride = {
  fileType?: string | null;
  subType?: { type: string } | null;
};

// Resolve the block type for a document notification, honoring `subType` so
// that e.g. a markdown doc with `subType: { type: 'task' }` routes to the
// 'task' block alias instead of raw 'md'. Prefers the live entity's fields
// over the notification-metadata snapshot when provided.
function safeDocumentContentToBlockName(
  content: NotificationEntityOverride,
  entity?: NotificationEntityOverride
) {
  return itemToBlockName({
    type: 'document',
    fileType: entity?.fileType ?? content.fileType ?? undefined,
    subType: entity?.subType ?? content.subType ?? undefined,
  } as ItemLike);
}

function resolveBlockCommentParamName(type: BlockName | BlockAlias) {
  const resolved = resolveBlockAlias(type);
  if (resolved === 'md' || resolved === 'spreadsheet')
    return MD_URL_PARAMS.commentId;
  if (resolved === 'pdf') return PDF_URL_PARAMS.annotationId;
}

type DocumentCommentLocation = {
  blockName: BlockName | BlockAlias;
  commentId: string;
  params?: Record<string, string>;
};

/**
 * The block and params that open a document at one of its comments, the same
 * target a copied comment link resolves to.
 */
export function documentCommentLocation(
  commentId: string,
  document: NotificationEntityOverride
): DocumentCommentLocation {
  const blockName = safeDocumentContentToBlockName(document);
  const commentParamName = resolveBlockCommentParamName(blockName);
  return {
    blockName,
    commentId,
    params: commentParamName ? { [commentParamName]: commentId } : undefined,
  };
}

/** Route-owned comment requests, including repeats, never require a block handle. */
export function documentCommentLocationUpdates(
  documentId: string,
  location: DocumentCommentLocation
): Record<string, SplitSearchUpdate> | undefined {
  return match(resolveBlockAlias(location.blockName))
    .with('md', 'spreadsheet', () =>
      markdownLocationUpdates(documentId, { commentId: location.commentId })
    )
    .with('pdf', () =>
      pdfLocationUpdates(documentId, {
        [PDF_URL_PARAMS.annotationId]: location.commentId,
      })
    )
    .otherwise(() => undefined);
}

/** {@link documentCommentLocation} for a document comment notification. */
export function getDocumentCommentLocation(
  notification: UnifiedNotification,
  entity?: NotificationEntityOverride
): DocumentCommentLocation | undefined {
  const meta = notification.notification_metadata;
  if (
    meta.tag !== 'mentioned_in_document_comment' &&
    meta.tag !== 'replied_to_document_comment_thread' &&
    meta.tag !== 'commented_on_document'
  ) {
    return undefined;
  }

  return documentCommentLocation(meta.content.commentId.toString(), {
    fileType: entity?.fileType ?? meta.content.fileType,
    subType: entity?.subType ?? meta.content.subType,
  });
}
