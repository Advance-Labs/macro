import {
  createSearchParamsCodec,
  type SplitSearchUpdate,
} from '@app/lib/split-router';
import { z } from 'zod';

export const canvasDetailSearch = {
  namespace: 'canvas-detail',
  schema: z.object({
    documentId: z.string(),
    x: z.string(),
    y: z.string(),
    scale: z.string(),
    seek: z.string(),
  }),
  defaults: { documentId: '', x: '', y: '', scale: '', seek: '' },
};
export const canvasDetailSearchCodec =
  createSearchParamsCodec(canvasDetailSearch);

/** Preserve the parameter aliases used by copied Canvas locations. */
export function canvasLocationUpdates(
  documentId: string,
  params: Record<string, string>,
  seek = crypto.randomUUID()
): Record<string, SplitSearchUpdate> {
  const coordinate = (...keys: string[]) =>
    keys
      .map((key) => params[key])
      .find(
        (value) =>
          value !== undefined &&
          value.trim() !== '' &&
          Number.isFinite(Number(value))
      ) ?? '';
  const target = canvasDetailSearchCodec.serialize({
    documentId,
    x: coordinate('x', 'canvas_x'),
    y: coordinate('y', 'canvas_y'),
    scale: coordinate('s', 'scale', 'canvas_scale'),
    seek,
  });
  return {
    [canvasDetailSearch.namespace]: (current) => {
      const {
        documentId: _documentId,
        x: _x,
        y: _y,
        scale: _scale,
        seek: _seek,
        ...rest
      } = current ?? {};
      return { ...rest, ...target };
    },
  };
}
