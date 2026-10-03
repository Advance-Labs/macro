import { blockMetadata } from '@app/lib/constants/block-metadata';
import { defineBlock, type ExtractLoadType, LoadErrors } from '@core/block';
import { ok } from 'neverthrow';

import { CompanyBlockAdapter } from './component/CompanyBlockAdapter';

export const definition = defineBlock({
  ...blockMetadata.company,
  name: 'company',
  component: CompanyBlockAdapter,
  liveTrackingEnabled: false,
  async load(source, _intent) {
    if (source.type === 'dss') {
      return ok({ id: source.id });
    }
    return LoadErrors.MISSING;
  },
});

export type CompanyData = ExtractLoadType<(typeof definition)['load']>;
