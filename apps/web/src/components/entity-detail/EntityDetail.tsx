import { ImageBlock } from '@app/features/block-image/ImageBlock';
import type { ImageDocumentData } from '@app/features/block-image/queries/image-document';
import {
  CanvasDetail,
  type CanvasDetailContext,
} from '@app/features/drive-view/views/CanvasDetail';
import {
  CodeDetail,
  type CodeDetailContext,
} from '@app/features/drive-view/views/CodeDetail';
import {
  MarkdownDetail,
  type MarkdownDetailContext,
} from '@app/features/drive-view/views/MarkdownDetail';
import {
  PdfDetail,
  type PdfDetailContext,
} from '@app/features/drive-view/views/PdfDetail';
import {
  UnknownDetail,
  type UnknownDetailContext,
} from '@app/features/drive-view/views/UnknownDetail';
import {
  VideoDetail,
  type VideoDetailContext,
} from '@app/features/drive-view/views/VideoDetail';
import { getChannelEntityTarget } from '@app/features/next-soup/utils';
import type { BlockAlias, BlockName } from '@app/lib/constants/block-registry';
import { fileTypeToBlockName } from '@app/lib/constants/file-metadata';
import type { MarkdownDocumentKind } from '@block-md/types';
import {
  ChannelDetail,
  type ChannelDetailContext,
  ChannelDetailTopBar,
} from '@channel/Channel/ChannelDetail';
import type { ChannelTargetRequest } from '@channel/Channel/ChannelSurface';
import { useGlobalBlockOrchestrator } from '@components/app/GlobalAppState';
import { PreviewPanel } from '@components/app/PreviewPanel';
import { previewBlockTarget } from '@components/app/previewTarget';
import { useSplitPanelOrThrow } from '@components/app/split-layout/layoutUtils';
import { ENABLE_VIDEO_BLOCK } from '@core/constant/featureFlags';
import {
  children,
  createMemo,
  type JSX,
  Match,
  Switch,
  untrack,
} from 'solid-js';
import type { EntityDetailTarget } from './entity-detail-target';
import { FileEntityDetail } from './FileEntityDetail';
import type { FileDetailContext } from './file-detail-context';

type DocumentDetailContext =
  | MarkdownDetailContext
  | CodeDetailContext
  | CanvasDetailContext
  | FileDetailContext<ImageDocumentData>
  | VideoDetailContext
  | PdfDetailContext
  | UnknownDetailContext;

export type EntityDetailContext =
  | ({ type: 'document' } & DocumentDetailContext)
  | ({ type: 'channel' } & ChannelDetailContext);

export type EntityDetailProps = {
  target: EntityDetailTarget;
  previewHeaderLeading?: JSX.Element;
  navigationRequest?: number | string;
  children?: (context: EntityDetailContext) => JSX.Element;
};

function ResolvedEntityDetailChildren(props: {
  render?: EntityDetailProps['children'];
  context: EntityDetailContext;
  fallback?: JSX.Element;
}) {
  const resolved = children(() => {
    if (props.render) return props.render(props.context);
    return props.fallback;
  });
  return <>{resolved()}</>;
}

function PreviewPanelEntityDetail(props: EntityDetailProps) {
  const orchestrator = useGlobalBlockOrchestrator();
  const panel = useSplitPanelOrThrow();

  return (
    <PreviewPanel
      target={previewBlockTarget(props.target)}
      orchestrator={orchestrator}
      splitPanelContext={panel}
      headerLeading={props.previewHeaderLeading}
    />
  );
}

function markdownKind(kind: BlockName | BlockAlias): MarkdownDocumentKind {
  if (kind === 'task' || kind === 'snippet' || kind === 'skill') return kind;
  return 'document';
}

export function entityDetailBlockType(
  target: EntityDetailTarget
): BlockName | BlockAlias | undefined {
  if (target.type !== 'document') return;

  const subType = target.subType?.type;
  const blockType = fileTypeToBlockName(
    subType === 'task' || subType === 'snippet' || subType === 'skill'
      ? subType
      : target.fileType
  );
  if (blockType === 'video' && !ENABLE_VIDEO_BLOCK) return 'unknown';
  if (
    blockType === 'md' ||
    blockType === 'task' ||
    blockType === 'snippet' ||
    blockType === 'skill' ||
    blockType === 'canvas' ||
    blockType === 'spreadsheet' ||
    blockType === 'code' ||
    blockType === 'csv' ||
    blockType === 'image' ||
    blockType === 'pdf' ||
    blockType === 'video' ||
    blockType === 'unknown'
  ) {
    return blockType;
  }
}

type ChannelDetailTarget = {
  channelId: string;
  target: ChannelTargetRequest | undefined;
  fallbackName?: string;
};

function channelDetailTarget(
  target: EntityDetailTarget
): ChannelDetailTarget | undefined {
  if (
    target.type !== 'channel' &&
    target.type !== 'channel_message' &&
    target.type !== 'channel_thread'
  ) {
    return undefined;
  }
  const clickTarget = getChannelEntityTarget(target);
  return {
    channelId: target.type === 'channel' ? target.id : target.channelId,
    target:
      clickTarget?.kind === 'message'
        ? {
            kind: 'message',
            messageId: clickTarget.messageId,
            threadId: clickTarget.threadId,
          }
        : clickTarget,
    fallbackName: target.fallbackName,
  };
}

export function EntityDetail(props: EntityDetailProps) {
  const documentTarget = () =>
    props.target.type === 'document' ? props.target : undefined;
  const blockType = () => entityDetailBlockType(props.target);
  // Resolved once per entry (untracked): a channel row's aim must not shift
  // and re-scroll when its notifications reconcile to read — PreviewPanel
  // applied the same rule by keying navigation on the explicit target only.
  const channelTarget = createMemo(() => {
    const target = props.target;
    return untrack(() => channelDetailTarget(target));
  });
  const renderChildren = (context: DocumentDetailContext) => (
    <ResolvedEntityDetailChildren
      render={props.children}
      context={{ type: 'document', ...context }}
    />
  );

  return (
    <Switch>
      <Match
        when={
          blockType() === 'md' ||
          blockType() === 'task' ||
          blockType() === 'snippet' ||
          blockType() === 'skill'
            ? documentTarget()
            : undefined
        }
      >
        {(target) => (
          <MarkdownDetail
            documentId={target().id}
            kind={markdownKind(blockType()!)}
            fallbackName={target().fallbackName}
          >
            {(context) => <>{renderChildren(context)}</>}
          </MarkdownDetail>
        )}
      </Match>
      <Match when={blockType() === 'code' || blockType() === 'csv'}>
        <CodeDetail documentId={props.target.id}>
          {(context) => <>{renderChildren(context)}</>}
        </CodeDetail>
      </Match>
      <Match when={blockType() === 'canvas'}>
        <CanvasDetail documentId={props.target.id}>
          {(context) => <>{renderChildren(context)}</>}
        </CanvasDetail>
      </Match>
      <Match when={blockType() === 'image'}>
        <ImageBlock documentId={props.target.id}>
          {(context, content) => (
            <FileEntityDetail
              documentId={context.documentId}
              data={context.data}
              documentMetadata={context.documentMetadata}
              userAccessLevel={context.userAccessLevel}
              onDownload={context.download}
              content={content}
            >
              {(context) => <>{renderChildren(context)}</>}
            </FileEntityDetail>
          )}
        </ImageBlock>
      </Match>
      <Match when={blockType() === 'video'}>
        <VideoDetail documentId={props.target.id}>
          {(context) => <>{renderChildren(context)}</>}
        </VideoDetail>
      </Match>
      <Match when={blockType() === 'pdf'}>
        <PdfDetail documentId={props.target.id}>
          {(context) => <>{renderChildren(context)}</>}
        </PdfDetail>
      </Match>
      <Match when={blockType() === 'unknown'}>
        <UnknownDetail documentId={props.target.id}>
          {(context) => <>{renderChildren(context)}</>}
        </UnknownDetail>
      </Match>
      <Match when={channelTarget()}>
        {(channel) => (
          <ChannelDetail
            channelId={channel().channelId}
            target={channel().target}
            navigationRequest={props.navigationRequest}
            fallbackName={channel().fallbackName}
          >
            {(context) => (
              <ResolvedEntityDetailChildren
                render={props.children}
                context={{ type: 'channel', ...context }}
                fallback={
                  <ChannelDetailTopBar
                    channelId={context.channelId}
                    fallbackName={channel().fallbackName}
                  />
                }
              />
            )}
          </ChannelDetail>
        )}
      </Match>
      <Match when={true}>
        <PreviewPanelEntityDetail
          target={props.target}
          previewHeaderLeading={props.previewHeaderLeading}
        />
      </Match>
    </Switch>
  );
}
