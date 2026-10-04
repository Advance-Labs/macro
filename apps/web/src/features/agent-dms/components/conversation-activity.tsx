import { Button } from '@ui';
import { Show } from 'solid-js';

export function ConversationActivity(props: {
  running: boolean;
  queued: number;
  failed?: 'failed' | 'stopped' | 'interrupted';
  canRetry: boolean;
  pending: boolean;
  onStop: () => void;
  onRetry: () => void;
}) {
  return (
    <Show when={props.running || props.queued > 0 || props.failed}>
      <div class="flex items-center justify-between gap-3 px-4 py-2 text-sm text-ink-muted">
        <div role="status" aria-live="polite">
          <Show when={props.running}>Agent is working…</Show>
          <Show when={props.queued > 0}>
            <span class="ml-2">{props.queued} queued</span>
          </Show>
          <Show when={!props.running && props.failed}>
            {props.failed === 'interrupted'
              ? 'Connection interrupted. Review any completed actions before retrying.'
              : props.failed === 'stopped'
                ? 'The last attempt was stopped.'
                : 'The last attempt could not finish.'}
          </Show>
        </div>
        <Show when={props.running}>
          <Button
            variant="ghost"
            size="sm"
            disabled={props.pending}
            onClick={props.onStop}
          >
            Stop
          </Button>
        </Show>
        <Show when={!props.running && props.failed && props.canRetry}>
          <Button
            variant="ghost"
            size="sm"
            disabled={props.pending}
            onClick={props.onRetry}
          >
            Retry message
          </Button>
        </Show>
      </div>
    </Show>
  );
}
