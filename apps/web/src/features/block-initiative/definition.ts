import { blockMetadata } from '@app/lib/constants/block-metadata';
import { defineBlock, type ExtractLoadType, LoadErrors } from '@core/block';
import { ok } from 'neverthrow';
import { lazy } from 'solid-js';

/** A task project (initiative); `project` is a folder block. */
export const definition = defineBlock({
  ...blockMetadata.initiative,
  name: 'initiative',
  component: lazy(() => import('./component/InitiativeBlock')),
  async load(source, _intent) {
    if (source.type === 'dss') {
      return ok({ id: source.id });
    }
    return LoadErrors.MISSING;
  },
});

export type InitiativeData = ExtractLoadType<(typeof definition)['load']>;
