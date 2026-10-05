import {
  createSearchParams,
  useOwnsSearchNamespace,
} from '@app/lib/split-router';
import {
  previewOwnsRoute,
  useMaybePreviewPanel,
} from '@components/app/preview-panel-context';
import { type Accessor, createEffect, on } from 'solid-js';
import { chatDetailSearch } from '../chat-route';

/** Deliver the latest owner-local message request to the existing readiness queue. */
export function createChatRouteNavigation(
  chatId: Accessor<string>,
  navigate: (params: Record<string, string> | undefined) => void,
  enabled = true
) {
  if (!enabled) return;
  const ownsSearch = useOwnsSearchNamespace(chatDetailSearch.namespace);
  const preview = useMaybePreviewPanel();
  const [search] = createSearchParams(chatDetailSearch);
  let routeOwnsTarget = false;
  createEffect(
    on(
      () => [
        search.chatId,
        search.messageId,
        search.seek,
        chatId(),
        ownsSearch(),
      ],
      (current, previous) => {
        if (
          previous &&
          current.every((value, index) => value === previous[index])
        )
          return;
        if (
          !ownsSearch() ||
          (preview && !previewOwnsRoute(preview, 'chat', chatId())) ||
          search.chatId !== chatId() ||
          !search.messageId
        ) {
          if (routeOwnsTarget) {
            routeOwnsTarget = false;
            if (!preview) navigate(undefined);
          }
          return;
        }
        routeOwnsTarget = true;
        navigate({ message_id: search.messageId });
      }
    )
  );
}
