import { useBlockEntityCommands } from '@app/features/next-soup/actions';
import { DEFAULT_CHAT_NAME } from '@app/lib/constants/block-metadata';
import { useGlobalNotificationSource } from '@components/app/GlobalAppState';
import { SidePanel } from '@components/app/side-panel';
import { useBlockId } from '@core/block';
import { DocumentBlockContainer } from '@core/component/DocumentBlockContainer';
import { HotkeyScope, useHotkeyScopeOrCreate } from '@core/hotkey/HotkeyScope';
import { useBlockDocumentName } from '@core/util/currentBlockDocumentName';
import { DebouncedNotificationReadMarker } from '@notifications';
import { Show } from 'solid-js';
import { chatBlockData } from '../signal/chatBlockData';
import { Chat } from './Chat';
import { ChatSidePanelSections } from './sidepanel/ChatSidePanelSections';

export default function ChatBlock() {
  const blockId = useBlockId();
  const [attachHotkeyScope, hotkeyScope] = useHotkeyScopeOrCreate('chat');
  useBlockEntityCommands({
    id: () => blockId,
    scopeId: () => hotkeyScope,
  });
  const notificationSource = useGlobalNotificationSource();
  const name = useBlockDocumentName(DEFAULT_CHAT_NAME);

  return (
    <HotkeyScope scope={hotkeyScope}>
      <DocumentBlockContainer
        title={name()}
        attachHotkeyScope={attachHotkeyScope}
      >
        <div class="size-full" tabIndex={-1}>
          <DebouncedNotificationReadMarker
            notificationSource={notificationSource}
            entity={{ type: 'chat', id: blockId }}
          />
          <SidePanel.Layout defaultOpen={false} floating>
            <ChatSidePanelSections />
            <Show when={chatBlockData()}>
              {(data) => (
                <Chat data={data()} permissionOptions={{ edit: false }} />
              )}
            </Show>
          </SidePanel.Layout>
        </div>
      </DocumentBlockContainer>
    </HotkeyScope>
  );
}
