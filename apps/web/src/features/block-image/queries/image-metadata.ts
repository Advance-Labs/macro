import { entityKeys } from '@queries/storage/keys';
import type { DocumentMetadata } from '@service-storage/generated/schemas/documentMetadata';
import type { QueryClient } from '@tanstack/solid-query';
import { from } from 'solid-js';

/** Reuse the side panel's metadata cache for rename updates without another storage query. */
export function createImageMetadataSource(
  client: QueryClient,
  documentId: string,
  metadata: DocumentMetadata
) {
  const queryKey = entityKeys.documentMetadata(documentId).queryKey;
  client.setQueryData(queryKey, metadata);
  const query = client.getQueryCache().find({ queryKey, exact: true });

  return from<DocumentMetadata>((set) => {
    set(metadata);
    return client.getQueryCache().subscribe((event) => {
      if (event.type !== 'updated' || event.query !== query) return;
      const latest = client.getQueryData<DocumentMetadata>(queryKey);
      if (latest) set(latest);
    });
  });
}
