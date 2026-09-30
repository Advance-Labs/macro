import { ResponsivePermissionsBadge } from '@components/app/ResponsiveBlockToolbar';
import {
  SplitHeaderLeft,
  SplitHeaderRight,
} from '@components/app/split-layout/components/SplitHeader';
import { BlockItemSplitLabel } from '@components/app/split-layout/components/SplitLabel';
import {
  SplitToolbarLeft,
  SplitToolbarRight,
} from '@components/app/split-layout/components/SplitToolbar';
import { useBlockId } from '@core/block';
import { BlockLiveIndicators } from '@core/component/LiveIndicators';
import { toast } from '@core/component/Toast/Toast';
import { ShareTrigger } from '@core/component/TopBar/ShareButton';
import { useShareModal } from '@core/component/TopBar/shareModal';
import { useCanEdit, useGetPermissions } from '@core/signal/permissions';
import SparkleIcon from '@phosphor/sparkle.svg';
import type {
  DatabaseDetail,
  DatabaseTableDetail,
} from '@service-storage/databases';
import { getEntityGraphqlClient } from '@service-storage/graphql-soup';
import { Button } from '@ui';
import { Show } from 'solid-js';
import { DatabaseTitle } from '../components/database-title';
import { renameDatabase } from '../queries/rename-database';
import { DatabasePageActions } from '../views/database-page-actions';
import { TableTabs } from './TableTabs';

export function TopBar(props: {
  detail: DatabaseDetail | undefined;
  activeTable: DatabaseTableDetail | undefined;
  /** Opens the title for typing, as for a freshly created database. */
  autoFocusTitle: boolean;
  onTitleConfirm: () => void;
  onSelectTable: (tableId: string) => void;
  onDelete: () => Promise<void>;
  openingChat: boolean;
  onOpenChat: () => void;
}) {
  const databaseId = useBlockId();
  const canEdit = useCanEdit();
  const permissions = useGetPermissions();
  let editTitle: (() => void) | undefined;
  const name = () => props.detail?.database.name ?? 'Database';
  const openShare = useShareModal(() => {
    const detail = props.detail;
    if (!detail) return;
    return {
      id: databaseId,
      blockAlias: 'database',
      itemType: 'database',
      name: detail.database.name,
      owner: detail.database.owner_id,
      userPermissions: permissions(),
    };
  });
  const rename = (next: string) =>
    void renameDatabase(getEntityGraphqlClient(), databaseId, next).catch(() =>
      toast.failure('Could not rename this database.')
    );

  return (
    <>
      <SplitHeaderLeft>
        <BlockItemSplitLabel
          name={name}
          title={
            <Show when={props.detail}>
              <DatabaseTitle
                name={name()}
                canEdit={canEdit()}
                autoFocus={props.autoFocusTitle}
                onConfirm={props.onTitleConfirm}
                onEditReady={(edit) => (editTitle = edit)}
                onRename={rename}
              />
            </Show>
          }
        />
      </SplitHeaderLeft>
      <SplitHeaderRight>
        <BlockLiveIndicators />
        <div class="order-[1000] flex items-center gap-1">
          <ShareTrigger onClick={openShare} />
        </div>
      </SplitHeaderRight>
      <ResponsivePermissionsBadge />
      <Show when={props.detail}>
        {(detail) => (
          <>
            <SplitToolbarLeft class="min-w-0">
              <TableTabs
                databaseId={databaseId}
                tables={detail().tables}
                activeTableId={props.activeTable?.table.id}
                canEdit={canEdit()}
                onSelect={props.onSelectTable}
              />
            </SplitToolbarLeft>
            <SplitToolbarRight>
              <DatabasePageActions
                detail={detail()}
                table={props.activeTable}
                onRename={() => editTitle?.()}
                onDelete={props.onDelete}
                onImported={props.onSelectTable}
              />
              <Button
                variant="ghost"
                size="sm"
                class="gap-1.5 px-2 text-xs"
                disabled={props.openingChat}
                aria-label="Database AI"
                aria-busy={props.openingChat}
                onClick={props.onOpenChat}
              >
                <SparkleIcon class="size-4" />
                <span>AI</span>
              </Button>
            </SplitToolbarRight>
          </>
        )}
      </Show>
    </>
  );
}
