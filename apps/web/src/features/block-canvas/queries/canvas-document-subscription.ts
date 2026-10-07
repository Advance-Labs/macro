import { useEntitySubscription } from '@service-connection/client';
import type { Accessor } from 'solid-js';

/** Keep the browser-only subscription client out of pure document queries. */
export function useCanvasDocumentSubscription(documentId: Accessor<string>) {
  useEntitySubscription(() => ({
    entity_type: 'document',
    entity_id: documentId(),
  }));
}
