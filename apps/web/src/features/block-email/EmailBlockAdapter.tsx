import type {
  EmailThreadHost,
  EmailThreadSource,
} from '@app/features/email-thread/context/email-thread-context';
import { URL_PARAMS } from '@app/features/email-thread/core/location';
import { emailDetailSearch } from '@app/features/email-view/email-route';
import {
  createSearchParams,
  useOwnsSearchNamespace,
} from '@app/lib/split-router';
import {
  previewOwnsRoute,
  useMaybePreviewPanel,
} from '@components/app/preview-panel-context';
import {
  useCanAutofocusSplitContent,
  useSplitPanel,
} from '@components/app/split-layout/layoutUtils';
import { useHotkeyScope } from '@core/hotkey/HotkeyScope';
import { TOKENS } from '@core/hotkey/tokens';
import { registerScopeSignalHotkey } from '@core/hotkey/utils';
import { isTouchDevice } from '@core/mobile/isTouchDevice';
import { createMethodRegistration } from '@core/orchestrator';
import { blockElementSignal } from '@core/signal/blockElement';
import { blockHandleSignal } from '@core/signal/load';
import { useSearchParams } from '@solidjs/router';
import {
  type Accessor,
  createEffect,
  createSignal,
  on,
  onCleanup,
} from 'solid-js';
import { TopBar } from './component/TopBar';
import {
  EmailThreadHostView,
  type EmailThreadHostViewProps,
} from './EmailThreadHostView';
import { useEmailListNavigation } from './use-email-list-navigation';
import { registerEmailHotkeys } from './util/emailHotkeys';

export function EmailBlockAdapter(props: {
  title: string;
  threadId: Accessor<string>;
  source: EmailThreadSource;
  threadTransport: EmailThreadHostViewProps['threadTransport'];
}) {
  const preview = useMaybePreviewPanel();
  const [params] = useSearchParams();
  const [routeSearch] = createSearchParams(emailDetailSearch);
  const ownsSearch = useOwnsSearchNamespace(emailDetailSearch.namespace);
  const routeTarget = () =>
    ownsSearch() &&
    !!routeSearch.messageId &&
    (!preview || previewOwnsRoute(preview, 'email', props.threadId()));
  const rawTarget = params[URL_PARAMS.messageId];
  const previewMessageId = () => {
    const current = preview?.previewTarget();
    return current?.blockId === props.threadId()
      ? (current.params as Record<string, string> | undefined)?.[
          URL_PARAMS.messageId
        ]
      : undefined;
  };
  const [targetMessageId, setTargetMessageId] = createSignal(
    routeTarget()
      ? routeSearch.messageId
      : preview
        ? previewMessageId()
        : Array.isArray(rawTarget)
          ? rawTarget[0]
          : rawTarget
  );
  let routeOwnsTarget = routeTarget();
  const [targetRequest, setTargetRequest] = createSignal<string | undefined>(
    routeOwnsTarget
      ? routeSearch.seek
      : preview?.navigationRequest()?.toString()
  );
  const split = useSplitPanel();
  const listNavigation = useEmailListNavigation(props.threadId);
  const canAutofocus = useCanAutofocusSplitContent();
  const blockElement = blockElementSignal.get;
  const scope = useHotkeyScope();
  const hotkeyScope = () => scope;
  const focusContainer = () => blockElement()?.focus({ preventScroll: true });
  let targetTimer: ReturnType<typeof setTimeout> | undefined;
  createEffect(
    on(
      () => [
        previewMessageId(),
        preview?.navigationRequest(),
        routeTarget(),
        routeSearch.seek,
      ],
      () => {
        if (!preview || routeTarget()) return;
        setTargetMessageId(previewMessageId());
        setTargetRequest(preview.navigationRequest()?.toString());
      },
      { defer: true }
    )
  );
  createEffect(
    on(
      () => [
        routeTarget(),
        routeSearch.messageId,
        routeSearch.seek,
        ownsSearch(),
        preview?.routeOwner(),
      ],
      () => {
        if (!routeTarget()) {
          if (routeOwnsTarget) {
            routeOwnsTarget = false;
            if (!preview) {
              setTargetMessageId(undefined);
              setTargetRequest(undefined);
            }
          }
          return;
        }
        clearTimeout(targetTimer);
        routeOwnsTarget = true;
        setTargetMessageId(routeSearch.messageId);
        setTargetRequest(routeSearch.seek);
      },
      { defer: true }
    )
  );
  createMethodRegistration(blockHandleSignal.get, {
    goToLocationFromParams: (params: Record<string, unknown>) => {
      const id = params[URL_PARAMS.messageId];
      if (typeof id !== 'string' || !id) return;
      clearTimeout(targetTimer);
      routeOwnsTarget = false;
      setTargetMessageId(undefined);
      setTargetRequest(undefined);
      targetTimer = setTimeout(() => setTargetMessageId(id), 0);
    },
  });
  onCleanup(() => clearTimeout(targetTimer));
  let focused = false;
  createEffect(() => {
    if (focused || !canAutofocus || isTouchDevice() || !blockElement()) return;
    focusContainer();
    focused = true;
  });
  const host: EmailThreadHost = {
    listNavigation,
    targetMessageId,
    targetRequest,
    focusContainer,
    isActive: () => split?.isPanelActive() !== false,
    registerKeyboard: (handlers) => {
      registerEmailHotkeys(hotkeyScope(), handlers);
      registerScopeSignalHotkey(hotkeyScope, {
        hotkey: 'enter',
        description: 'Reply to message',
        keyDownHandler: handlers.activate,
        hotkeyToken: TOKENS.block.focus,
        hide: true,
      });
      registerScopeSignalHotkey(hotkeyScope, {
        hotkey: 'escape',
        description: 'Collapse or unselect message',
        keyDownHandler: handlers.cancel,
        hotkeyToken: TOKENS.email.cancelReply,
        hide: true,
      });
    },
  };

  return (
    <EmailThreadHostView
      title={props.title}
      threadId={props.threadId}
      source={props.source}
      threadTransport={props.threadTransport}
      host={host}
      topBar={({ createTask }) => (
        <TopBar
          id={props.threadId()}
          permissionOptions={{ edit: false }}
          title={props.title}
          onCreateTask={createTask}
        />
      )}
    />
  );
}
