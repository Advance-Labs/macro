import { type Accessor, createSignal, For, onMount, Show } from 'solid-js';
import { Portal } from 'solid-js/web';
import { IncomingPhoneCallCard } from '../components/incoming-phone-call-card';
import { usePhoneContext } from '../context/phone-context';
import {
  formatPhoneNumber,
  type IncomingPhoneCall,
  remotePartyLabel,
} from '../core/phone-call';
import {
  PhoneCallError,
  phoneCallErrorMessage,
} from '../core/phone-call-error';
import { createIncomingPhoneCalls } from '../primitives/incoming-phone-calls';
import { createPhoneCallSession } from '../primitives/phone-call-session';
import { ActivePhoneCall } from './active-phone-call';
import { DialerDialog, type DialerRequest } from './dialer-dialog';

/**
 * Everything phone calling shows app-wide: the dialer when asked for, calls
 * ringing for the viewer, and the call they are on.
 */
export function PhoneCallsView(props: {
  dialerRequest: Accessor<DialerRequest | null>;
  onDialerClose: () => void;
}) {
  const context = usePhoneContext();
  const session = createPhoneCallSession({
    operations: context.operations,
    media: context.media,
  });
  const [answering, setAnswering] = createSignal<string>();
  const [answerErrors, setAnswerErrors] = createSignal<Record<string, string>>(
    {}
  );

  async function answer(call: IncomingPhoneCall) {
    if (answering()) return;
    setAnswering(call.callId);
    setAnswerErrors((errors) =>
      Object.fromEntries(
        Object.entries(errors).filter(([callId]) => callId !== call.callId)
      )
    );
    try {
      await session.answer(call);
      incoming.resolve(call.callId);
    } catch (error) {
      if (error instanceof PhoneCallError && error.kind === 'gone') {
        incoming.resolve(call.callId);
        return;
      }
      setAnswerErrors((errors) => ({
        ...errors,
        [call.callId]: phoneCallErrorMessage(error),
      }));
    } finally {
      setAnswering(undefined);
    }
  }

  const incoming = createIncomingPhoneCalls({
    alerts: context.alerts,
    answer: (call) => void answer(call),
  });

  async function decline(call: IncomingPhoneCall) {
    incoming.resolve(call.callId);
    try {
      await context.operations.hangUp(call.callId);
    } catch {
      // The caller stops hearing a ring when they give up either way.
    }
  }

  /** Calls that started ringing before this page loaded. */
  async function syncIncoming() {
    try {
      for (const call of await context.operations.listIncoming())
        incoming.receive(call);
    } catch {
      // New calls still arrive over the websocket.
    }
  }

  context.subscribe((event) => {
    if (event.type === 'incoming') {
      incoming.receive(event.call);
      return;
    }
    incoming.update(event.callId, event.leg);
    session.update(event.callId, event.leg);
  });

  onMount(() => void syncIncoming());

  return (
    <>
      <Show when={props.dialerRequest()} keyed>
        {(request) => (
          <DialerDialog
            request={request}
            session={session}
            onClose={props.onDialerClose}
          />
        )}
      </Show>
      <Show when={incoming.calls().length > 0 || session.state().t !== 'idle'}>
        <Portal>
          <div
            role="region"
            aria-label="Phone calls"
            class="pointer-events-none fixed bottom-[max(1rem,var(--safe-bottom,0px))] left-[max(1rem,env(safe-area-inset-left,0px))] z-toast-region flex max-h-[calc(100dvh-2rem)] w-80 max-w-[calc(100vw-2rem)] flex-col gap-3 overflow-y-auto touch:bottom-[calc(var(--mobile-content-inset-bottom,var(--safe-bottom,0px))+12px)]"
          >
            <For each={incoming.calls()}>
              {(call) => {
                const named = () => Boolean(call.contact?.name?.trim());
                return (
                  <IncomingPhoneCallCard
                    title={remotePartyLabel({
                      contact: call.contact,
                      remoteNumber: call.from,
                    })}
                    number={named() ? formatPhoneNumber(call.from) : undefined}
                    dialedNumber={
                      call.to ? formatPhoneNumber(call.to) : undefined
                    }
                    answering={answering() === call.callId}
                    endsCurrentCall={context.media.inAnyCall()}
                    error={answerErrors()[call.callId]}
                    onAnswer={() => void answer(call)}
                    onDecline={() => void decline(call)}
                  />
                );
              }}
            </For>
            <ActivePhoneCall session={session} />
          </div>
        </Portal>
      </Show>
    </>
  );
}
