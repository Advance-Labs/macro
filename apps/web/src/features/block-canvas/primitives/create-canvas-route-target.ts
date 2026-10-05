import {
  createSearchParams,
  useOwnsSearchNamespace,
} from '@app/lib/split-router';
import { type Accessor, createMemo } from 'solid-js';
import { canvasDetailSearch } from '../canvas-route';

export function createCanvasRouteTarget(
  documentId: Accessor<string>,
  enabled = true
) {
  if (!enabled) return () => undefined;
  const ownsSearch = useOwnsSearchNamespace(canvasDetailSearch.namespace);
  const [search] = createSearchParams(canvasDetailSearch);
  const request = createMemo(
    () =>
      [
        ownsSearch(),
        documentId(),
        search.documentId,
        search.x,
        search.y,
        search.scale,
        search.seek,
      ] as const,
    undefined,
    {
      equals: (previous, next) =>
        previous?.every((value, index) => value === next[index]) ?? false,
    }
  );
  return createMemo<Record<string, string> | undefined>(() => {
    const [owns, id, targetId, x, y, scale] = request();
    if (!owns || targetId !== id || (!x && !y && !scale)) return;
    return { ...(x && { x }), ...(y && { y }), ...(scale && { scale }) };
  });
}
