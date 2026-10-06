import PhoneIcon from '@phosphor/phone.svg';
import PhoneIncomingIcon from '@phosphor/phone-incoming.svg';
import { Avatar, Button, Tooltip } from '@ui';
import { Show } from 'solid-js';

/** A phone call ringing for the viewer. It never steals focus. */
export function IncomingPhoneCallCard(props: {
  /** The caller: a contact name or a formatted number. */
  title: string;
  /** The formatted number, when `title` is a name. */
  number?: string;
  /** Which of the viewer's numbers was dialed, formatted. */
  dialedNumber?: string;
  answering: boolean;
  /** Answering will end the call the viewer is on. */
  endsCurrentCall: boolean;
  error?: string;
  onAnswer: () => void;
  onDecline: () => void;
}) {
  return (
    <section
      aria-label={`Incoming phone call from ${props.title}`}
      class="pointer-events-auto rounded-xl border border-edge-muted bg-menu p-4 text-ink glass [--color-surface:var(--color-menu)]"
    >
      <div class="flex items-center gap-3">
        <Avatar
          size="lg"
          class="size-12 bg-accent-bg text-accent ring-2 ring-accent/40 ring-offset-2 ring-offset-menu"
        >
          <Avatar.Fallback>
            <PhoneIncomingIcon aria-hidden="true" class="size-6" />
          </Avatar.Fallback>
        </Avatar>
        <div class="min-w-0 flex-1" role="status">
          <div class="mb-1 text-xs font-semibold uppercase tracking-wide text-accent">
            Incoming phone call
          </div>
          <Tooltip label={props.title} class="block">
            <h2 class="truncate text-base font-semibold">{props.title}</h2>
          </Tooltip>
          <Show when={props.number}>
            <p class="truncate text-sm text-ink-muted tabular-nums">
              {props.number}
            </p>
          </Show>
          <Show when={props.dialedNumber}>
            <p class="truncate text-xs text-ink-muted tabular-nums">
              To {props.dialedNumber}
            </p>
          </Show>
        </div>
      </div>
      <Show when={props.error}>
        <p role="alert" class="mt-3 text-sm text-failure">
          {props.error}
        </p>
      </Show>
      <div class="mt-4 flex gap-2">
        <Button
          variant="danger"
          size="lg"
          class="flex-1"
          aria-label={`Decline call from ${props.title}`}
          disabled={props.answering}
          onClick={props.onDecline}
        >
          Decline
        </Button>
        <Button
          variant="success"
          size="lg"
          class="flex-1"
          aria-label={`Answer call from ${props.title}`}
          disabled={props.answering}
          onClick={props.onAnswer}
        >
          <PhoneIcon aria-hidden="true" class="size-4" />
          {props.answering
            ? 'Answering…'
            : props.endsCurrentCall
              ? 'End & answer'
              : 'Answer'}
        </Button>
      </div>
    </section>
  );
}
