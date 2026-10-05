import { useTauri } from '@macro/tauri';
import { Tooltip } from '@ui/components/Tooltip';
import { type ParentProps, Show } from 'solid-js';

export function DesktopRecordingIndicator() {
  const tauri = useTauri();
  const recordingId = () => tauri?.desktopWindowChrome().recordingId;

  return (
    <Show when={recordingId()}>
      {(id) => (
        <Tooltip
          label={`Memory recording ${id()}`}
          placement="right"
          tabIndex={0}
        >
          <span
            role="status"
            aria-label={`Memory recording ${id()}`}
            class="inline-flex h-5 items-center gap-1.5 text-[10px] font-medium tracking-wide text-ink-muted"
          >
            <span aria-hidden="true" class="size-1.5 rounded-full bg-failure" />
            REC
          </span>
        </Tooltip>
      )}
    </Show>
  );
}

/** Routes without the app rail still need room for native controls and dragging. */
export function DesktopWindowHeader() {
  const tauri = useTauri();

  return (
    <Show when={tauri?.desktopWindowChrome().enabled}>
      <div
        data-ui="desktop-window-header"
        data-tauri-drag-region
        class="flex h-[44px] shrink-0 select-none items-center pl-[80px]"
      >
        <DesktopRecordingIndicator />
      </div>
    </Show>
  );
}

/** Keep standalone routes mounted while the native window opts into the shell. */
export function DesktopWindowFrame(props: ParentProps) {
  const tauri = useTauri();
  const enabled = () => tauri?.desktopWindowChrome().enabled;

  return (
    <div class={enabled() ? 'flex h-dvh flex-col bg-panel' : 'contents'}>
      <DesktopWindowHeader />
      <div
        class={
          enabled()
            ? 'min-h-0 flex-1 overflow-y-auto [&>*]:max-h-full'
            : 'contents'
        }
      >
        {props.children}
      </div>
    </div>
  );
}
