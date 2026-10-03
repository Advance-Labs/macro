import { blockMetadata } from '@app/lib/constants/block-metadata';
import {
  defineBlock,
  type ExtractLoadType,
  LoadErrors,
  loadResult,
} from '@core/block';
import { storageServiceClient } from '@service-storage/client';
import { err, ok } from 'neverthrow';
import BlockCode from './component/Block';

export const definition = defineBlock({
  ...blockMetadata.code,
  name: 'code',
  component: BlockCode,
  async load(source, intent) {
    if (intent === 'preload') {
      return ok({
        type: 'preload',
        origin:
          source.type === 'preload'
            ? source.origin
            : source.type === 'gen'
              ? source.origin
              : source,
      });
    }
    if (source.type !== 'dss') return LoadErrors.INVALID;
    const document = await loadResult(
      storageServiceClient.getTextDocument({
        documentId: source.id,
      })
    );
    if (document.isErr()) return err(document.error);
    const result = document.value;
    return ok(result);
  },

  liveTrackingEnabled: true,
  syncServiceEnabled: false,
});

export type CodeData = ExtractLoadType<(typeof definition)['load']>;
