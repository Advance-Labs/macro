import { useBlockEntityCommands } from '@app/features/next-soup/actions';
import { FileSidePanelSections, SidePanel } from '@components/app/side-panel';
import { useBlockId } from '@core/block';
import { DocumentBlockContainer } from '@core/component/DocumentBlockContainer';
import { HotkeyScope, useHotkeyScopeOrCreate } from '@core/hotkey/HotkeyScope';
import { blockData } from '../signal/blockData';
import { TopBar } from './TopBar';
import { VideoContent } from './VideoContent';

export default function BlockVideo() {
  const documentId = useBlockId();
  const [attachHotkeyScope, hotkeyScope] = useHotkeyScopeOrCreate('video');
  useBlockEntityCommands({
    id: () => documentId,
    scopeId: () => hotkeyScope,
  });
  return (
    <HotkeyScope scope={hotkeyScope}>
      <DocumentBlockContainer attachHotkeyScope={attachHotkeyScope}>
        <div class="size-full select-none overscroll-none overflow-hidden flex flex-col relative">
          <SidePanel.Layout defaultOpen={false} floating>
            <FileSidePanelSections />
            <div class="flex size-full min-w-0 flex-col overflow-hidden">
              <div class="relative">
                <TopBar permissionOptions={{ edit: false }} />
              </div>
              <div class="w-full grow relative overflow-hidden">
                <VideoContent
                  videoUrl={blockData()?.videoUrl}
                  fileType={blockData()?.documentMetadata.fileType}
                />
              </div>
            </div>
          </SidePanel.Layout>
        </div>
      </DocumentBlockContainer>
    </HotkeyScope>
  );
}
