import {
  createSearchParams,
  useOwnsSearchNamespace,
} from '@app/lib/split-router';
import {
  previewOwnsRoute,
  useMaybePreviewPanel,
} from '@components/app/preview-panel-context';
import { type Accessor, createEffect, on } from 'solid-js';
import { URL_PARAMS } from '../constants';
import { markdownDetailSearch } from '../markdown-route';

/** Deliver route requests to the editor's existing queued location handler. */
export function createMarkdownRouteNavigation(
  documentId: Accessor<string>,
  navigate: (params: Record<string, string>) => void
) {
  const [search] = createSearchParams(markdownDetailSearch);
  const ownsSearch = useOwnsSearchNamespace(markdownDetailSearch.namespace);
  const preview = useMaybePreviewPanel();
  const routeTarget = () =>
    ownsSearch() &&
    (!preview || previewOwnsRoute(preview, 'md', documentId())) &&
    search.documentId === documentId() &&
    !!(search.nodeId || search.commentId);
  createEffect(
    on(
      () =>
        [
          search.nodeId,
          search.commentId,
          search.seek,
          search.documentId,
          documentId(),
          routeTarget(),
        ] as const,
      ([nodeId, commentId]) => {
        if (!routeTarget()) return;
        const params: Record<string, string> = {};
        if (nodeId) params[URL_PARAMS.nodeId] = nodeId;
        if (commentId) params[URL_PARAMS.commentId] = commentId;
        if (nodeId || commentId) navigate(params);
      }
    )
  );
}
