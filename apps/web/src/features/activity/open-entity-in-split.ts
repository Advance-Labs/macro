import { projectRouteId } from '@app/features/projects/core/route';
import { fileTypeToBlockName } from '@app/lib/constants/file-metadata';
import { globalSplitManager } from '@app/signal/splitLayout';
import { useSplitPanel } from '@components/app/split-layout/layoutUtils';
import { openDocument } from '@core/component/LexicalMarkdown/component/core/BlockLink';
import { toast } from '@core/component/Toast/Toast';
import { enableProjects, isFeatureEnabled } from '@core/constant/featureFlags';
import type { OpenEntityTarget } from './context/activity-context';

/** The app's `onOpen` for activity rows: open the entity in the split layout. */
export function openEntityInSplit({
  block,
  id,
  params,
  newSplit,
}: OpenEntityTarget): void {
  if (block.toLowerCase() === 'initiative') {
    if (!isFeatureEnabled(enableProjects)) return;
    globalSplitManager()?.openWithSplit(
      {
        type: 'component',
        id: projectRouteId({
          id,
          section: 'overview',
          discussionId: params?.discussion_id,
        }),
      },
      { preferNewSplit: newSplit }
    );
    return;
  }
  const manager = globalSplitManager();
  const owner = manager?.findOpenView({ type: fileTypeToBlockName(block), id });
  const sourceOwner = useSplitPanel()?.handle.id ?? manager?.activeSplitId();
  let notified = false;
  const notifyReused = () => {
    if (notified) return;
    notified = true;
    toast.alert('Content already open');
  };
  const result = openDocument(block, id, params, newSplit, () => {
    if (owner && owner.owner !== sourceOwner) notifyReused();
  });
  if (result?.status === 'reused' && result.owner !== result.sourceOwner) {
    notifyReused();
  }
}
