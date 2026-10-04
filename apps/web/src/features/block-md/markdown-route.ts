import {
  createSearchParamsCodec,
  type SplitSearchUpdate,
} from '@app/lib/split-router';
import { z } from 'zod';

export const markdownDetailSearch = {
  namespace: 'markdown-detail',
  schema: z.object({
    documentId: z.string(),
    nodeId: z.string(),
    commentId: z.string(),
    seek: z.string(),
  }),
  defaults: { documentId: '', nodeId: '', commentId: '', seek: '' },
};

export const markdownDetailSearchCodec =
  createSearchParamsCodec(markdownDetailSearch);

/** One complete document location per click, without replacing list search. */
export function markdownLocationUpdates(
  documentId: string,
  location: { nodeId?: string; commentId?: string },
  seek = crypto.randomUUID()
): Record<string, SplitSearchUpdate> {
  const target = markdownDetailSearchCodec.serialize({
    ...markdownDetailSearch.defaults,
    documentId,
    ...location,
    seek,
  });
  return {
    [markdownDetailSearch.namespace]: (current) => {
      const {
        documentId: _documentId,
        nodeId: _nodeId,
        commentId: _commentId,
        seek: _seek,
        ...rest
      } = current ?? {};
      return { ...rest, ...target };
    },
  };
}
