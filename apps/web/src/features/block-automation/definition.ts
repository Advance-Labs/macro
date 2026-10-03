import { blockMetadata } from '@app/lib/constants/block-metadata';
import { defineBlock, type ExtractLoadType, LoadErrors } from '@core/block';
import { ok } from 'neverthrow';

import { Automation } from './component/Automation';

export const definition = defineBlock({
  ...blockMetadata.automation,
  name: 'automation',
  component: Automation,
  async load(source, intent) {
    if (source.type === 'dss') {
      if (intent === 'preload') {
        return ok({
          type: 'preload',
          origin: source,
        });
      }
      return ok({ scheduleId: source.id });
    }
    return LoadErrors.INVALID;
  },
  liveTrackingEnabled: false,
});

export type AutomationData = ExtractLoadType<(typeof definition)['load']>;
