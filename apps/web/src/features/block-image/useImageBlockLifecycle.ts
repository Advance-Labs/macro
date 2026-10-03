import { useAnalytics } from '@app/lib/analytics/analytics-context';
import { useSplitPanel } from '@components/app/split-layout/layoutUtils';
import { registerHotkey } from '@core/hotkey/hotkeys';
import { setCopiedItem } from '@core/state/clipboard';
import { useQueryClient } from '@queries/client';
import { track } from '@queries/history/track-block-opened';
import { type Accessor, createEffect, on } from 'solid-js';

/** Run under the loaded image owner so tracking and shortcuts follow its lifetime. */
export function useImageBlockLifecycle(documentId: Accessor<string>) {
  const analytics = useAnalytics();
  const client = useQueryClient();
  const panel = useSplitPanel();

  createEffect(
    on(documentId, (documentId) => {
      track({
        itemId: documentId,
        blockName: 'image',
        client: () => client,
      });
      analytics.pageView('image');
      analytics.track('open_entity', {
        entityType: 'image',
        entityId: documentId,
      });
    })
  );

  if (panel) {
    registerHotkey({
      scopeId: panel.splitHotkeyScope,
      hotkey: 'cmd+c',
      description: 'Copy image reference',
      hide: true,
      keyDownHandler: () => {
        setCopiedItem({ type: 'document', id: documentId() });
        return false;
      },
    });
  }
}
