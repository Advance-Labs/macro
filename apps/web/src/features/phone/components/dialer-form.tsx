import BackspaceIcon from '@phosphor/backspace.svg';
import PhoneIcon from '@phosphor/phone.svg';
import { ActionDialogShell, Button, Input } from '@ui';
import { Show } from 'solid-js';
import { Dialpad } from './dialpad';

/**
 * The dialer's contents for an action dialog: the number being typed, a
 * keypad, and the call button. The host owns the dialog and the call.
 */
export function DialerForm(props: {
  value: string;
  onInput: (value: string) => void;
  onKey: (key: string) => void;
  onBackspace: () => void;
  onCall: () => void;
  canCall: boolean;
  calling: boolean;
  /** The number the callee will see, formatted. */
  callerId?: string;
  /** Something to know before calling, e.g. that it ends the current call. */
  notice?: string;
  error?: string;
  /** Offered with the error when the viewer's plan doesn't cover the call. */
  onOpenPlan?: () => void;
  inputRef?: (element: HTMLInputElement) => void;
}) {
  return (
    <form
      class="flex min-h-0 flex-col"
      aria-busy={props.calling}
      onSubmit={(event) => {
        event.preventDefault();
        if (props.canCall && !props.calling) props.onCall();
      }}
    >
      <ActionDialogShell.Body class="space-y-4">
        <ActionDialogShell.Header>
          <ActionDialogShell.Title>Call a phone number</ActionDialogShell.Title>
          <Show when={props.callerId}>
            <ActionDialogShell.Description>
              Calling from {props.callerId}
            </ActionDialogShell.Description>
          </Show>
        </ActionDialogShell.Header>
        <div class="flex items-center gap-2">
          <Input
            ref={props.inputRef}
            type="tel"
            inputMode="tel"
            autocomplete="tel"
            aria-label="Phone number"
            placeholder="(555) 234-5678 or +44 20 7946 0958"
            value={props.value}
            readOnly={props.calling}
            aria-invalid={props.error ? true : undefined}
            onInput={(event) => props.onInput(event.currentTarget.value)}
            class="text-center text-lg tabular-nums"
            size="xl"
          />
          <Button
            type="button"
            size="icon-lg"
            variant="ghost"
            label="Delete last digit"
            disabled={props.calling || props.value.length === 0}
            onClick={props.onBackspace}
          >
            <BackspaceIcon />
          </Button>
        </div>
        <Dialpad onKey={props.onKey} disabled={props.calling} />
        <Show when={props.notice}>
          <p class="text-sm text-ink-muted">{props.notice}</p>
        </Show>
        <Show when={props.error}>
          <div class="flex items-start justify-between gap-3">
            <p role="alert" class="text-sm text-failure">
              {props.error}
            </p>
            <Show when={props.onOpenPlan}>
              {(onOpenPlan) => (
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={() => onOpenPlan()()}
                >
                  Phone plan
                </Button>
              )}
            </Show>
          </div>
        </Show>
      </ActionDialogShell.Body>
      <ActionDialogShell.Footer>
        <Button
          type="submit"
          variant="success"
          size="lg"
          fullWidth
          disabled={!props.canCall || props.calling}
        >
          <PhoneIcon aria-hidden="true" class="size-4" />
          {props.calling ? 'Calling…' : 'Call'}
        </Button>
      </ActionDialogShell.Footer>
    </form>
  );
}
