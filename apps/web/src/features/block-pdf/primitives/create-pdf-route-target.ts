import {
  createSearchParams,
  useOwnsSearchNamespace,
} from '@app/lib/split-router';
import { type Accessor, createMemo } from 'solid-js';
import { URL_PARAMS } from '../constants';
import { pdfDetailSearch } from '../pdf-route';
import type { LocationBlockParams } from '../signal/location';

export function createPdfRouteTarget(documentId: Accessor<string>) {
  const ownsSearch = useOwnsSearchNamespace(pdfDetailSearch.namespace);
  const [search] = createSearchParams(pdfDetailSearch);
  const request = createMemo(
    () =>
      [
        ownsSearch(),
        documentId(),
        search.documentId,
        search.annotationId,
        search.page,
        JSON.stringify(search.highlightTerms),
        search.snippet,
        search.query,
        search.pageNumber,
        search.yPos,
        search.x,
        search.width,
        search.height,
        search.seek,
      ] as const,
    undefined,
    {
      equals: (previous, next) =>
        previous?.every((value, index) => value === next[index]) ?? false,
    }
  );
  return createMemo<LocationBlockParams | undefined>(() => {
    const [
      owns,
      id,
      targetId,
      annotationId,
      page,
      terms,
      snippet,
      query,
      pageNumber,
      yPos,
      x,
      width,
      height,
    ] = request();
    if (!owns || (targetId && targetId !== id)) return;
    if (annotationId) return { [URL_PARAMS.annotationId]: annotationId };
    if (page >= 0)
      return {
        [URL_PARAMS.searchPage]: String(page),
        [URL_PARAMS.searchHighlightTerms]: terms,
        [URL_PARAMS.searchSnippet]: snippet,
        [URL_PARAMS.searchRawQuery]: query,
      };
    if (!pageNumber) return;
    return {
      [URL_PARAMS.pageNumber]: pageNumber,
      ...(yPos && { [URL_PARAMS.yPos]: yPos }),
      ...(x && { [URL_PARAMS.x]: x }),
      ...(width && { [URL_PARAMS.width]: width }),
      ...(height && { [URL_PARAMS.height]: height }),
    };
  });
}
