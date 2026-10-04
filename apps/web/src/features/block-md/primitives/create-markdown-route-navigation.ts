import { createSearchParams } from '@app/lib/split-router';
import { type Accessor, createEffect, on } from 'solid-js';
import { URL_PARAMS } from '../constants';
import { markdownDetailSearch } from '../markdown-route';

/** Deliver route requests to the editor's existing queued location handler. */
export function createMarkdownRouteNavigation(
  documentId: Accessor<string>,
  navigate: (params: Record<string, string>) => void
) {
  const [search] = createSearchParams(markdownDetailSearch);
  createEffect(
    on(
      () =>
        [
          search.nodeId,
          search.commentId,
          search.seek,
          search.documentId,
          documentId(),
        ] as const,
      ([nodeId, commentId]) => {
        if (search.documentId && search.documentId !== documentId()) return;
        const params: Record<string, string> = {};
        if (nodeId) params[URL_PARAMS.nodeId] = nodeId;
        if (commentId) params[URL_PARAMS.commentId] = commentId;
        if (nodeId || commentId) navigate(params);
      }
    )
  );
}
