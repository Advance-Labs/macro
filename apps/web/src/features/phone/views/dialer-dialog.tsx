import { ActionDialogShell, Dialog } from '@ui';
import { createSignal, onMount } from 'solid-js';
import { DialerForm } from '../components/dialer-form';
import { usePhoneContext } from '../context/phone-context';
import { formatPhoneNumber, looksDialable } from '../core/phone-call';
import { phoneCallErrorMessage } from '../core/phone-call-error';
import type { PhoneCallSession } from '../primitives/phone-call-session';

/** A request to show the dialer, optionally placing the call right away. */
export type DialerRequest = {
  /** The number to start from, as typed or in E.164. */
  number: string;
  /** Call immediately, e.g. from a contact's Call button. */
  autoDial: boolean;
};

export function DialerDialog(props: {
  request: DialerRequest;
  session: PhoneCallSession;
  onClose: () => void;
}) {
  const context = usePhoneContext();
  const [value, setValue] = createSignal(props.request.number);
  const [error, setError] = createSignal<string>();
  let input: HTMLInputElement | undefined;

  const settings = () => context.settings.settings();
  const onPhoneCall = () => props.session.state().t === 'live';
  const canCall = () =>
    settings()?.dialingEnabled === true &&
    !onPhoneCall() &&
    looksDialable(value());
  const notice = () => {
    if (context.settings.isError())
      return 'Could not load your phone settings. Calling may not work.';
    if (settings()?.dialingEnabled === false)
      return "Phone calling isn't set up for your workspace yet.";
    if (onPhoneCall())
      return 'Hang up your current phone call to place another.';
    if (context.media.inAnyCall())
      return 'Calling will end the call you are on.';
    return undefined;
  };
  const callerId = () => {
    const number = settings()?.callerId;
    return number ? formatPhoneNumber(number) : undefined;
  };

  function edit(next: string) {
    setValue(next);
    setError(undefined);
  }

  async function call() {
    setError(undefined);
    try {
      await props.session.dial(value());
      props.onClose();
    } catch (failure) {
      setError(phoneCallErrorMessage(failure));
      input?.focus();
    }
  }

  onMount(() => {
    // A contact's Call button already chose the number; the server decides
    // whether it can be called.
    if (props.request.autoDial && value()) void call();
  });

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !props.session.starting()) props.onClose();
      }}
      position="center"
      class="w-96"
      visibleScrim
    >
      <ActionDialogShell>
        <DialerForm
          value={value()}
          onInput={edit}
          onKey={(key) => {
            edit(value() + key);
            input?.focus();
          }}
          onBackspace={() => {
            edit(value().slice(0, -1));
            input?.focus();
          }}
          onCall={() => void call()}
          canCall={canCall()}
          calling={props.session.starting()}
          callerId={callerId()}
          notice={notice()}
          error={error()}
          inputRef={(element) => {
            input = element;
          }}
        />
      </ActionDialogShell>
    </Dialog>
  );
}
