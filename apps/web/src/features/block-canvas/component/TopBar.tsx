import {
  ChatWithAgentIcon,
  openChatWithAgent,
} from '@app/features/chat/ChatWithAgentButton';
import { useAnalytics } from '@app/lib/analytics/analytics-context';
import type { BlockTool } from '@components/app/ResponsiveBlockToolbar';
import {
  ResponsiveBlockToolbar,
  ResponsivePermissionsBadge,
} from '@components/app/ResponsiveBlockToolbar';
import type { FileOperation } from '@components/app/split-layout/components/SplitFileMenu';
import {
  SplitHeaderLeft,
  SplitHeaderRight,
} from '@components/app/split-layout/components/SplitHeader';
import { StaticSplitLabel } from '@components/app/split-layout/components/SplitLabel';
import { LiveIndicators } from '@core/component/LiveIndicators';
import {
  getPermissions,
  hasPermissions,
  Permissions,
} from '@core/component/SharePermissions';
import {
  getShareDrawerRecipientInput,
  ShareTrigger,
} from '@core/component/TopBar/ShareButton';
import { useShareModal } from '@core/component/TopBar/shareModal';
import { ENABLE_LIVE_INDICATORS } from '@core/constant/featureFlags';
import { useUserId } from '@core/context/user';
import { useUserIndicators } from '@core/state/liveIndicators';
import { buildSimpleEntityUrl } from '@core/util/url';
import { useCopyLink } from '@core/util/useCopyLink';
import { createRenameDssEntityMutation } from '@entity';
import { downloadFile } from '@filesystem/download';
import IconShared from '@icon/share.svg';
import DownloadSimple from '@phosphor/download-simple.svg';
import { useItemRawName } from '@queries/preview';
import type { AccessLevel } from '@service-storage/generated/schemas/accessLevel';
import type { DocumentMetadata } from '@service-storage/generated/schemas/documentMetadata';
import { formatDocumentName } from '@service-storage/util/filename';
import { createCallback } from '@solid-primitives/rootless';
import type { Accessor } from 'solid-js';
import { onMount, Show } from 'solid-js';
import { URL_PARAMS } from '../constants';
import { useToolManager } from '../signal/toolManager';
import { useRenderState } from '../store/RenderState';
import type { CanvasDocumentChrome } from './CanvasDocument';

export type CanvasTopBarProps = {
  documentId: string;
  documentMetadata: DocumentMetadata;
  userAccessLevel: AccessLevel;
  sourceFile: Blob;
  savedFile: Accessor<Blob | undefined>;
  hotkeyScope: string;
  chrome: CanvasDocumentChrome;
};

export function TopBar(props: CanvasTopBarProps) {
  const analytics = useAnalytics();
  const permissions = () => getPermissions(props.userAccessLevel);
  const isOwner = () => hasPermissions(permissions(), Permissions.OWNER);
  const updatedName = useItemRawName(() => ({
    type: 'document',
    id: props.documentId,
  }));
  const fileName = () =>
    updatedName() || props.documentMetadata.documentName || 'Unknown Filename';
  const downloadName = () =>
    formatDocumentName(
      fileName(),
      props.documentMetadata.fileType ?? 'canvas',
      { caseInsensitiveSuffix: true }
    );
  const rename = createRenameDssEntityMutation();
  const entity = () => ({
    type: 'document' as const,
    id: props.documentId,
    name: fileName(),
    fileType: 'canvas' as const,
    ownerId: props.documentMetadata.owner,
  });
  const toolManager =
    props.chrome.mode === 'next' ? undefined : useToolManager();
  const getLocation =
    props.chrome.mode === 'next'
      ? props.chrome.location!
      : useRenderState().getLocation;
  const openShare = useShareModal(() => ({
    id: props.documentId,
    blockAlias: 'canvas',
    itemType: 'document',
    name: fileName(),
    userPermissions: permissions(),
    owner: props.documentMetadata.owner,
  }));
  const copyEntityLink = useCopyLink();
  const indicators = useUserIndicators(() => props.documentId);
  const userId = useUserId();

  let ref!: HTMLDivElement;
  onMount(() => {
    toolManager?.ignoreMouseEvents(ref);
  });

  const downloadDocument = createCallback(async () => {
    downloadFile(props.savedFile() ?? props.sourceFile, downloadName());
    analytics.track('download', { blockType: 'canvas' });
  });

  const copyLink = () => {
    const location = getLocation();
    const params = {
      [URL_PARAMS.x]: location.x.toString(),
      [URL_PARAMS.y]: location.y.toString(),
      [URL_PARAMS.s]: location.s.toString(),
    };
    copyEntityLink(
      buildSimpleEntityUrl({ type: 'canvas', id: props.documentId }, params)
    );
    analytics.track('copy_share_link', { blockType: 'canvas' });
  };

  const ops: FileOperation[] = [
    { op: 'copy' },
    { op: 'rename' },
    { op: 'moveToProject' },
    {
      group: 'file',
      label: 'Download',
      icon: DownloadSimple,
      action: downloadDocument,
    },
    { op: 'delete' },
  ];

  const tools: BlockTool[] = [
    {
      label: 'Ask Macro',
      icon: ChatWithAgentIcon,
      action: () =>
        openChatWithAgent({
          type: 'document',
          id: props.documentId,
          name: fileName(),
          fileType: 'canvas',
        }),
    },
    {
      group: 'sharing',
      label: 'Share',
      icon: IconShared,
      action: openShare,
      buttonComponent: () => (
        <ShareTrigger
          id={props.documentId}
          blockType="canvas"
          hotkeyScope={props.hotkeyScope}
          onClick={openShare}
          copyLink={copyLink}
        />
      ),
      focusTarget: getShareDrawerRecipientInput,
    },
  ];

  return (
    <div ref={ref}>
      <SplitHeaderLeft>
        <StaticSplitLabel
          label={fileName()}
          iconType="canvas"
          onRename={
            isOwner()
              ? (newName) => rename.mutate({ entity: entity(), newName })
              : undefined
          }
        />
      </SplitHeaderLeft>
      <SplitHeaderRight>
        <Show when={ENABLE_LIVE_INDICATORS}>
          <div class="-order-1 touch:hidden">
            <LiveIndicators
              userIds={indicators() ?? []}
              currentUserId={userId()}
            />
          </div>
        </Show>
      </SplitHeaderRight>
      <ResponsivePermissionsBadge permissions={permissions()} />
      <ResponsiveBlockToolbar
        tools={tools}
        ops={ops}
        id={props.documentId}
        itemType="document"
        name={fileName()}
        entity={entity()}
        entityKind="canvas"
        permissions={permissions()}
      />
    </div>
  );
}
