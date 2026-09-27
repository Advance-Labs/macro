import { prepareEmailThreads } from '@app/features/email-thread/preparation-adapter';
import { useEmailRenderCache } from '@app/lib/email-render-cache/session';
import { type Accessor, createEffect, on, onCleanup } from 'solid-js';
import { createPreparationWindow } from './preparation-window';

/** Translate reactive navigation values and dispose with the owning view. */
export function usePrepareEmailNeighbors(
  ids: Accessor<readonly string[]>,
  focusedId: Accessor<string | undefined>
) {
  const cache = useEmailRenderCache();
  const preparation = createPreparationWindow((service, id, priority) =>
    prepareEmailThreads(service, [id], priority)
  );
  createEffect(
    on([cache, ids, focusedId], ([service, ordered, focused]) =>
      preparation.update(service, ordered, focused)
    )
  );
  onCleanup(() => preparation.dispose());
}
