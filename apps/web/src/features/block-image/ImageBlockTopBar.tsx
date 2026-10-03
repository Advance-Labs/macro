import { DocumentTitleHoverCard } from '@app/components/entity-detail/DocumentTitleHoverCard';
import { FileEntityDetail } from '@app/components/entity-detail/FileEntityDetail';
import type { FileDetailContext } from '@app/components/entity-detail/file-detail-context';
import { useBlockEntityCommands } from '@app/features/next-soup/actions';
import { SidePanel } from '@components/app/side-panel';
import {
  type FileOperation,
  SplitFileMenu,
} from '@components/app/split-layout/components/SplitFileMenu';
import {
  SplitHeaderLeft,
  SplitHeaderRight,
} from '@components/app/split-layout/components/SplitHeader';
import {
  SplitHeaderBadge,
  SplitTitleFileMenu,
  StaticSplitLabel,
} from '@components/app/split-layout/components/SplitLabel';
import { useSplitPanelOrThrow } from '@components/app/split-layout/layoutUtils';
import { ENTITY_ICON_CONFIGS } from '@core/component/EntityIcon';
import {
  getPermissions,
  hasPermissions,
  Permissions,
} from '@core/component/SharePermissions';
import {
  getShareDrawerRecipientInput,
  ShareTrigger,
} from '@core/component/TopBar/ShareButton';
import { useDocumentShareModal } from '@core/component/TopBar/shareModal';
import { buildEntityData } from '@entity';
import IconShared from '@icon/share.svg';
import { cn } from '@ui';
import { Show } from 'solid-js';
import { ImageBlock } from './ImageBlock';

/** Standalone and preview chrome uses loaded image data, not ambient block state. */
export function ImageBlockTopBar(props: {
  documentId: string;
  context: FileDetailContext<unknown>;
}) {
  const panel = useSplitPanelOrThrow();
  const metadata = () => props.context.documentMetadata;
  const name = () => metadata().documentName || 'Untitled';
  const permissions = () => getPermissions(props.context.userAccessLevel);
  const canEdit = () => hasPermissions(permissions(), Permissions.CAN_EDIT);
  const canComment = () =>
    hasPermissions(permissions(), Permissions.CAN_COMMENT);
  const openShare = useDocumentShareModal(() => ({
    documentId: props.documentId,
    blockAlias: 'image',
  }));
  const tools = [
    {
      group: 'sharing' as const,
      label: 'Share',
      icon: IconShared,
      action: openShare,
      focusTarget: getShareDrawerRecipientInput,
    },
  ];
  const operations = (): FileOperation[] => [
    { op: 'rename' },
    { op: 'copy' },
    { op: 'moveToProject' },
    ...(props.context.operations ?? []),
    { op: 'delete' },
  ];

  useBlockEntityCommands({
    id: () => props.documentId,
    scopeId: () => panel.splitHotkeyScope,
    resolveEntity: () =>
      buildEntityData({
        id: props.documentId,
        name: name(),
        blockName: 'image',
        ownerId: metadata().owner,
        projectId: metadata().projectId ?? undefined,
      }),
  });

  return (
    <>
      <SplitHeaderLeft>
        <DocumentTitleHoverCard
          documentId={props.documentId}
          name={name()}
          ownerId={metadata().owner}
          createdAt={metadata().createdAt}
          updatedAt={metadata().updatedAt}
        >
          <StaticSplitLabel
            label={name()}
            iconType="image"
            colorIcon
            badges={
              <span
                class={cn(
                  'shrink-0 rounded px-1 py-0.5 text-xxs font-mono font-medium uppercase leading-none',
                  ENTITY_ICON_CONFIGS.image.background,
                  ENTITY_ICON_CONFIGS.image.foreground
                )}
              >
                {metadata().fileType}
              </span>
            }
          />
        </DocumentTitleHoverCard>
      </SplitHeaderLeft>
      <SplitHeaderRight>
        <Show when={!canEdit()}>
          <SplitHeaderBadge
            text={canComment() ? 'comment only' : 'viewer'}
            tooltip={canComment() ? 'Comment Only' : 'View Only'}
          />
        </Show>
        <SidePanel.HeaderActionsOutlet />
        <ShareTrigger
          onClick={openShare}
          id={props.documentId}
          blockType="image"
          hotkeyScope={panel.splitHotkeyScope}
        />
        <SidePanel.Toggle />
      </SplitHeaderRight>
      <SplitTitleFileMenu>
        <SplitFileMenu
          id={props.documentId}
          name={name()}
          itemType="document"
          entityKind="image"
          permissions={permissions()}
          ops={operations()}
          tools={tools}
        />
      </SplitTitleFileMenu>
    </>
  );
}

/** Compatibility routes and popovers mount the feature directly in existing split chrome. */
export function StandaloneImageBlock(props: { documentId: string }) {
  return (
    <SidePanel.Root persistKey="image" defaultOpen={false} floating>
      <ImageBlock documentId={props.documentId}>
        {(context, content) => (
          <FileEntityDetail
            documentId={context.documentId}
            data={context.data}
            documentMetadata={context.documentMetadata}
            userAccessLevel={context.userAccessLevel}
            onDownload={context.download}
            content={content}
          >
            {(detailContext) => (
              <ImageBlockTopBar
                documentId={context.documentId}
                context={detailContext}
              />
            )}
          </FileEntityDetail>
        )}
      </ImageBlock>
    </SidePanel.Root>
  );
}
