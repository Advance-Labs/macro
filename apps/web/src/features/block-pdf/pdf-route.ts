import {
  createSearchParamsCodec,
  type SplitSearchUpdate,
} from '@app/lib/split-router';
import { z } from 'zod';

import { URL_PARAMS } from './constants';
import type { LocationBlockParams } from './signal/location';
export const pdfDetailSearch = {
  namespace: 'pdf-detail',
  schema: z.object({
    documentId: z.string(),
    page: z.number().int().min(-1),
    highlightTerms: z.array(z.string()),
    snippet: z.string(),
    query: z.string(),
    seek: z.string(),
    annotationId: z.string(),
    pageNumber: z.string(),
    yPos: z.string(),
    x: z.string(),
    width: z.string(),
    height: z.string(),
  }),
  defaults: {
    documentId: '',
    page: -1,
    highlightTerms: [] as string[],
    snippet: '',
    query: '',
    seek: '',
    annotationId: '',
    pageNumber: '',
    yPos: '',
    x: '',
    width: '',
    height: '',
  },
};

export const pdfDetailSearchCodec = createSearchParamsCodec(pdfDetailSearch);

export const PDF_LOCATION_FIELDS = Object.keys(pdfDetailSearch.defaults);

/** Replace the complete PDF target, keeping search pages zero-based. */
export function pdfLocationUpdates(
  documentId: string,
  params: LocationBlockParams,
  seek = crypto.randomUUID()
): Record<string, SplitSearchUpdate> {
  let highlightTerms: string[] = [];
  try {
    const parsed = JSON.parse(params[URL_PARAMS.searchHighlightTerms] ?? '[]');
    highlightTerms = z.array(z.string()).parse(parsed);
  } catch {
    // Invalid search metadata does not discard a valid annotation or page.
  }
  const page = Number(params[URL_PARAMS.searchPage]);
  const target = pdfDetailSearchCodec.serialize({
    ...pdfDetailSearch.defaults,
    documentId,
    page: Number.isInteger(page) && page >= 0 ? page : -1,
    highlightTerms,
    snippet: params[URL_PARAMS.searchSnippet] ?? '',
    query: params[URL_PARAMS.searchRawQuery] ?? '',
    annotationId: params[URL_PARAMS.annotationId] ?? '',
    pageNumber: params[URL_PARAMS.pageNumber] ?? '',
    yPos: params[URL_PARAMS.yPos] ?? '',
    x: params[URL_PARAMS.x] ?? '',
    width: params[URL_PARAMS.width] ?? '',
    height: params[URL_PARAMS.height] ?? '',
    seek,
  });
  return {
    [pdfDetailSearch.namespace]: (current) => ({
      ...Object.fromEntries(
        Object.entries(current ?? {}).filter(
          ([field]) => !PDF_LOCATION_FIELDS.includes(field)
        )
      ),
      ...target,
    }),
  };
}
