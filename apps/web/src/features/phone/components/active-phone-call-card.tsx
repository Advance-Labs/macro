import DotsNineIcon from '@phosphor/dots-nine.svg';
import MicrophoneIcon from '@phosphor/microphone.svg';
import MicrophoneSlashIcon from '@phosphor/microphone-slash.svg';
import PhoneDisconnectIcon from '@phosphor/phone-disconnect.svg';
import { Button, cn } from '@ui';
import { Show } from 'solid-js';
import { Dialpad } from './dialpad';

/**
 * The call the viewer is on, docked above other content: who is on the other
 * end, where the call is, and the controls a phone has.
 */
export function ActivePhoneCallCard(props: {
  /** The other party: a contact name or a formatted number. */
  title: string;
  /** The formatted number, when `title` is a name. */
  number?: string;
  /** Where the call is: `Calling…`, an elapsed time, or how it ended. */
  status: string;
  /** Both sides are talking; the status shows the call's length. */
  connected: boolean;
  ended: boolean;
  muted: boolean;
  keypadOpen: boolean;
  /** Digits sent this call, echoed so the caller can check them. */
  sentDigits: string;
  onToggleMute: () => void;
  onToggleKeypad: () => void;
  onKey: (key: string) => void;
  onHangUp: () => void;
  /** Open the CRM contact; omitted when the caller is unknown. */
  onOpenContact?: () => void;
}) {
  return (
    <section
      aria-label={`Phone call with ${props.title}`}
      class="pointer-events-auto rounded-xl border border-edge-muted bg-menu p-4 text-ink glass [--color-surface:var(--color-menu)]"
    >
      <div class="flex items-start gap-3">
        <div class="min-w-0 flex-1">
          <Show
            when={props.onOpenContact}
            fallback={
              <h2 class="truncate text-base font-semibold">{props.title}</h2>
            }
          >
            {(open) => (
              <h2 class="truncate text-base font-semibold">
                <button
                  type="button"
                  class="max-w-full truncate rounded outline-none focus-visible:ring-2 focus-visible:ring-edge-focus not-touch:hover:underline"
                  onClick={open()}
                >
                  {props.title}
                </button>
              </h2>
            )}
          </Show>
          <Show when={props.number}>
            <p class="truncate text-sm text-ink-muted tabular-nums">
              {props.number}
            </p>
          </Show>
        </div>
        <p
          role="status"
          class={cn(
            'shrink-0 text-sm tabular-nums',
            props.connected && !props.ended ? 'text-success' : 'text-ink-muted'
          )}
        >
          {props.status}
        </p>
      </div>
      <Show when={props.keypadOpen && !props.ended}>
        <div class="mt-3">
          <p
            aria-live="polite"
            class="mb-2 h-5 truncate text-center font-mono text-sm tracking-widest text-ink-muted"
          >
            {props.sentDigits}
          </p>
          <Dialpad onKey={props.onKey} />
        </div>
      </Show>
      <div class="mt-3 flex items-center justify-center gap-3">
        <Button
          size="icon-lg"
          variant="ghost"
          label={props.muted ? 'Unmute microphone' : 'Mute microphone'}
          tooltipPlacement="top"
          aria-pressed={props.muted}
          disabled={props.ended}
          onClick={props.onToggleMute}
        >
          <Show when={props.muted} fallback={<MicrophoneIcon />}>
            <MicrophoneSlashIcon />
          </Show>
        </Button>
        <Button
          size="icon-lg"
          variant="ghost"
          label={props.keypadOpen ? 'Hide keypad' : 'Keypad'}
          tooltipPlacement="top"
          aria-pressed={props.keypadOpen}
          disabled={props.ended}
          onClick={props.onToggleKeypad}
        >
          <DotsNineIcon />
        </Button>
        <Button
          size="icon-lg"
          variant="danger"
          label="Hang up"
          tooltipPlacement="top"
          disabled={props.ended}
          onClick={props.onHangUp}
        >
          <PhoneDisconnectIcon />
        </Button>
      </div>
    </section>
  );
}
