import { IncomingMeetingInvitationsProvider } from '@app/features/meetings/incoming-meeting-invitations';
import { usePendingNotificationNavigationEffect } from '@app/features/notifications/PendingNotificationNavigationEffect';
import { SearchProvider } from '@app/features/soup/search/context';
import { useCalendarCache } from '@app/lib/queries/calendar/graphql/use-calendar-cache';
import { useInvalidateQueriesOnReconnect } from '@app/lib/queries/invalidate-on-reconnect';
import { useSoupBackfills } from '@app/lib/queries/soup/backfill';
import { globalSplitManager } from '@app/signal/splitLayout';
import { GlobalAppStateProvider } from '@components/app/GlobalAppState';
import { ReactiveFavicon } from '@components/app/ReactiveFavicon';
import { ChatAttachmentsInit } from '@core/component/AI/signal/globalAttachments';
import { TeamContextProvider } from '@core/context/team';
import { useUserId } from '@core/context/user';
import { isNativeMobilePlatform } from '@core/mobile/isNativeMobilePlatform';
import { createBlockOrchestrator } from '@core/orchestrator';
import {
  createNotificationSource,
  type UnifiedNotification,
  useNotificationUpdates,
  usePlatformNotificationState,
} from '@notifications';
import { maybeHandlePlatformNotification } from '@notifications/notification-platform';
import { useChatRenameWebsocketSync } from '@queries/chat';
import { QuerySyncProvider } from '@queries/sync/SyncProvider';
import { MutationUndoProvider } from '@queries/undo';
import {
  useRefreshTrackedEntitiesOnFocus,
  useReopenTrackedEntitiesOnReconnect,
} from '@service-connection/client';
import { ws as connectionGatewayWebsocket } from '@service-connection/websocket';
import { type ParentProps, Show } from 'solid-js';

function ConfiguredGlobalAppStateProvider(props: ParentProps) {
  // Initialize global notification helpers
  const notifInterface = usePlatformNotificationState();
  useChatRenameWebsocketSync();
  useReopenTrackedEntitiesOnReconnect();
  useRefreshTrackedEntitiesOnFocus();

  if (isNativeMobilePlatform()) {
    useInvalidateQueriesOnReconnect();
  }

  const onNotification = (notification: UnifiedNotification) => {
    if (notifInterface === 'not-supported') return;
    const layoutManager = globalSplitManager();
    if (!layoutManager) return;
    maybeHandlePlatformNotification(
      notification,
      notifInterface,
      layoutManager
    );
  };
  const notificationSource = createNotificationSource(
    connectionGatewayWebsocket,
    onNotification
  );
  useNotificationUpdates(notificationSource);

  const blockOrchestrator = createBlockOrchestrator();
  usePendingNotificationNavigationEffect(notificationSource);

  return (
    <GlobalAppStateProvider
      notificationSource={notificationSource}
      blockOrchestrator={blockOrchestrator}
    >
      {props.children}
    </GlobalAppStateProvider>
  );
}

function SoupBackfillSideEffect(props: { userId: string }) {
  useSoupBackfills(props.userId);
  return null;
}

function CalendarCacheSideEffect() {
  useCalendarCache();
  return null;
}

/** Caches only the app's views read: soup backfill and the calendar cache. */
function UserCacheSideEffects() {
  const userId = useUserId();
  return (
    <Show when={userId()} keyed>
      {(id) => (
        <>
          <SoupBackfillSideEffect userId={id} />
          <CalendarCacheSideEffect />
        </>
      )}
    </Show>
  );
}

function QuerySyncProviderWithUserId() {
  const userId = useUserId();
  return <QuerySyncProvider userId={userId} />;
}

/**
 * State only the app's views use: notifications, undo, search, and the caches
 * and sync behind them. Auth, booking, and meeting pages render without it.
 */
export function AppProviders(props: ParentProps) {
  return (
    <TeamContextProvider>
      <ConfiguredGlobalAppStateProvider>
        <MutationUndoProvider>
          <SearchProvider>
            <IncomingMeetingInvitationsProvider>
              <QuerySyncProviderWithUserId />
              <UserCacheSideEffects />
              <ChatAttachmentsInit />
              <ReactiveFavicon />
              {props.children}
            </IncomingMeetingInvitationsProvider>
          </SearchProvider>
        </MutationUndoProvider>
      </ConfiguredGlobalAppStateProvider>
    </TeamContextProvider>
  );
}
