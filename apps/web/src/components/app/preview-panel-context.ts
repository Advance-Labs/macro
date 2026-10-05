import { createContextProvider } from '@solid-primitives/context';
import type {
  PreviewBlockTarget,
  PreviewPanelSelection,
} from './previewTarget';

/** Normalized feature and entity owned by a mounted route preview. */
export type PreviewRouteOwner = Pick<
  PreviewBlockTarget,
  'blockType' | 'blockId'
>;

/** A preview never inherits route search without an explicit matching owner. */
export function previewOwnsRoute(
  preview: ReturnType<typeof useMaybePreviewPanel>,
  blockType: PreviewBlockTarget['blockType'],
  blockId: string
) {
  const owner = preview?.routeOwner();
  return (
    owner?.blockType === blockType &&
    owner.blockId === blockId &&
    preview?.previewTarget().blockType === blockType &&
    preview.previewTarget().blockId === blockId
  );
}

/** Local preview capabilities read only at feature adapter boundaries. */
export const [PreviewPanelContext, useMaybePreviewPanel] =
  createContextProvider(
    (props: {
      previewTarget: PreviewBlockTarget;
      navigationRequest?: number | string;
      routeOwner?: PreviewRouteOwner;
      previewEntity?: PreviewPanelSelection;
      onFocusOut?: VoidFunction;
    }) => ({
      previewTarget: () => props.previewTarget,
      navigationRequest: () => props.navigationRequest,
      routeOwner: () => props.routeOwner,
      previewEntity: () => props.previewEntity,
      onFocusOut: () => props.onFocusOut?.(),
    })
  );
