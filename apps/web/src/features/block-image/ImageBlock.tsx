import {
  EntityLoadGate,
  LoadErrorPanel,
  toEntityLoadError,
} from '@core/component/EntityLoadGate';
import { downloadFile } from '@filesystem/download';
import { useQueryClient } from '@queries/client';
import { formatDocumentName } from '@service-storage/util/filename';
import { createResource, ErrorBoundary, type JSX, Show } from 'solid-js';
import { ImageContent } from './component/ImageContent';
import {
  type ImageDocumentData,
  loadImageDocument,
} from './queries/image-document';
import { createImageMetadataSource } from './queries/image-metadata';
import { useImageBlockLifecycle } from './useImageBlockLifecycle';

export type ImageBlockContext = Pick<
  ImageDocumentData,
  'documentMetadata' | 'userAccessLevel'
> & {
  documentId: string;
  data: ImageDocumentData;
  download: () => void;
};

type ImageBlockChildren = (
  context: ImageBlockContext,
  content: JSX.Element
) => JSX.Element;

function LoadedImageBlock(props: {
  documentId: string;
  data: ImageDocumentData;
  children?: ImageBlockChildren;
}) {
  useImageBlockLifecycle(() => props.documentId);
  const metadataSource = createImageMetadataSource(
    useQueryClient(),
    props.documentId,
    props.data.documentMetadata
  );
  const metadata = () => metadataSource() ?? props.data.documentMetadata;
  const context: ImageBlockContext = {
    get documentId() {
      return props.documentId;
    },
    get data() {
      return props.data;
    },
    get documentMetadata() {
      return metadata();
    },
    get userAccessLevel() {
      return props.data.userAccessLevel;
    },
    download: () => {
      downloadFile(
        props.data.file,
        formatDocumentName(
          metadata().documentName || 'download',
          metadata().fileType,
          {
            caseInsensitiveSuffix: true,
          }
        )
      );
    },
  };
  const content = (
    <ImageContent
      file={props.data.file}
      alt={metadata().documentName || 'Image'}
    />
  );
  return props.children ? props.children(context, content) : content;
}

/** Image routes and previews own their load and lifecycle without a legacy block instance. */
export function ImageBlock(props: {
  documentId: string;
  children?: ImageBlockChildren;
}) {
  const [document, { refetch }] = createResource(
    () => props.documentId,
    async (documentId) => ({
      documentId,
      data: await loadImageDocument(documentId),
    })
  );
  const loadedImage = () => {
    if (document.state !== 'ready' && document.state !== 'refreshing') return;
    const loaded = document.latest;
    return loaded?.documentId === props.documentId ? loaded : undefined;
  };
  const data = () => loadedImage()?.data;
  const retry = () => void refetch();

  return (
    <ErrorBoundary
      fallback={(_error, reset) => (
        <LoadErrorPanel
          title="Unable to display this image"
          onRetry={() => {
            reset();
            retry();
          }}
        />
      )}
    >
      <EntityLoadGate
        result={{
          data,
          error: () => toEntityLoadError(document.error),
          isPending: () => document.loading && data() === undefined,
        }}
        onRetry={retry}
        loadErrorTitle="Unable to load this image"
      >
        <Show when={loadedImage()} keyed>
          {(loaded) => (
            <LoadedImageBlock
              documentId={loaded.documentId}
              data={loaded.data}
              children={props.children}
            />
          )}
        </Show>
      </EntityLoadGate>
    </ErrorBoundary>
  );
}
