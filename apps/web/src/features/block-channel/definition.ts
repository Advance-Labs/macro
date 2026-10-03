import { blockMetadata } from '@app/lib/constants/block-metadata';
import { defineBlock, type ExtractLoadType, LoadErrors } from '@core/block';
import { ok } from 'neverthrow';

import { NewChannelBlockAdapter } from './component/NewChannelBlockAdapter';

export const definition = defineBlock({
  ...blockMetadata.channel,
  name: 'channel',
  component: NewChannelBlockAdapter,
  liveTrackingEnabled: true,
  async load(source, _intent) {
    if (source.type === 'dss') {
      return ok({ id: source.id });
    }
    return LoadErrors.MISSING;
  },
});

export type ChannelData = ExtractLoadType<(typeof definition)['load']>;
