import { blockMetadata } from '@app/lib/constants/block-metadata';
import {
  defineBlock,
  type ExtractLoadType,
  LoadErrors,
  loadResult,
} from '@core/block';
import { storageServiceClient } from '@service-storage/client';
import { err, ok } from 'neverthrow';
import BlockUnknown from './component/Block';

export const definition = defineBlock({
  ...blockMetadata.unknown,
  name: 'unknown',
  component: BlockUnknown,
  async load(source, intent) {
    if (source.type === 'dss') {
      const maybeDocument = await loadResult(
        storageServiceClient.getDocumentMetadata({ documentId: source.id })
      );

      if (intent === 'preload') {
        return ok({
          type: 'preload',
          origin: source,
        });
      }

      if (maybeDocument.isErr()) return err(maybeDocument.error);

      const documentResult = maybeDocument.value;

      const { documentMetadata, userAccessLevel } = documentResult;

      return ok({ documentMetadata, userAccessLevel });
    }
    return LoadErrors.INVALID;
  },
  liveTrackingEnabled: false,
});

export type UnknownFileData = ExtractLoadType<(typeof definition)['load']>;
