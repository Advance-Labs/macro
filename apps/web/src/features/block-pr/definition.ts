import { blockMetadata } from '@app/lib/constants/block-metadata';
import { defineBlock, type ExtractLoadType, LoadErrors } from '@core/block';
import { ok } from 'neverthrow';
import { lazy } from 'solid-js';

export const definition = defineBlock({
  ...blockMetadata.pr,
  name: 'pr',
  // Lazy so the Pierre/shiki diff stack stays out of the main chunk.
  component: lazy(() => import('./component/Block')),
  liveTrackingEnabled: false,
  async load(source, _intent) {
    if (source.type === 'dss') {
      if (!source.id) return LoadErrors.INVALID;
      return ok({ id: source.id });
    }
    return LoadErrors.MISSING;
  },
});

export type PrData = ExtractLoadType<(typeof definition)['load']>;
