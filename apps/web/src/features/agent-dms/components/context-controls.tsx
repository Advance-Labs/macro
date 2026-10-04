import { Button } from '@ui';
import { createSignal, Show } from 'solid-js';

export function ContextControls(props: {
  available: boolean;
  settingsChanged: boolean;
  busy: boolean;
  pending: boolean;
  onStartFresh: () => Promise<void>;
}) {
  const [confirming, setConfirming] = createSignal(false);
  const reset = async () => {
    try {
      await props.onStartFresh();
      setConfirming(false);
    } catch {
      // The owning mutation reports the failure; retain the confirmation.
    }
  };
  return (
    <Show when={props.available}>
      <div class="flex flex-wrap items-center justify-between gap-2 px-4 py-2 text-xs text-ink-muted">
        <span>
          {props.settingsChanged
            ? 'This agent has updated settings.'
            : 'Private conversation with this agent'}
        </span>
        <Show
          when={confirming()}
          fallback={
            <Button
              size="xs"
              variant="ghost"
              disabled={props.busy || props.pending}
              onClick={() => setConfirming(true)}
            >
              {props.settingsChanged ? 'Use updated settings' : 'Start fresh'}
            </Button>
          }
        >
          <p class="w-full">
            Start a new context with the latest agent settings? Your history
            stays here. Queued messages will be stopped.
          </p>
          <Button
            size="xs"
            variant="ghost"
            disabled={props.pending}
            onClick={() => setConfirming(false)}
          >
            Keep conversation
          </Button>
          <Button
            size="xs"
            variant="strong"
            disabled={props.busy || props.pending}
            onClick={() => void reset()}
          >
            Start fresh
          </Button>
        </Show>
      </div>
    </Show>
  );
}
