import { useBlockEntityCommands } from '@app/features/next-soup/actions';
import { FileSidePanelSections, SidePanel } from '@components/app/side-panel';
import { useBlockId } from '@core/block';
import { DocumentBlockContainer } from '@core/component/DocumentBlockContainer';
import { blockHotkeyScopeSignal } from '@core/signal/blockElement';
import { blockData } from '../signal/blockData';
import { TopBar } from './TopBar';
import { VideoContent } from './VideoContent';

export default function BlockVideo() {
  const documentId = useBlockId();
  useBlockEntityCommands({
    id: () => documentId,
    scopeId: blockHotkeyScopeSignal.get,
  });
  return (
    <DocumentBlockContainer>
      <div class="size-full select-none overscroll-none overflow-hidden flex flex-col relative">
        <SidePanel.Layout defaultOpen={false} floating>
          <FileSidePanelSections />
          <div class="flex size-full min-w-0 flex-col overflow-hidden">
            <div class="relative">
              <TopBar />
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
  );
}
