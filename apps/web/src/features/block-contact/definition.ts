import { blockMetadata } from '@app/lib/constants/block-metadata';
import { defineBlock, type ExtractLoadType, LoadErrors } from '@core/block';
import { ok } from 'neverthrow';

import { ContactBlockAdapter } from './component/ContactBlockAdapter';

export const definition = defineBlock({
  ...blockMetadata.contact,
  name: 'contact',
  component: ContactBlockAdapter,
  liveTrackingEnabled: false,
  async load(source, _intent) {
    if (source.type === 'dss') {
      return ok({ id: source.id });
    }
    return LoadErrors.MISSING;
  },
});

export type ContactData = ExtractLoadType<(typeof definition)['load']>;
