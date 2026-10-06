import { createSignal, Show } from 'solid-js';
import { ActivePhoneCallCard } from '../components/active-phone-call-card';
import { usePhoneContext } from '../context/phone-context';
import {
  dtmfCode,
  formatCallDuration,
  formatPhoneNumber,
  remotePartyLabel,
} from '../core/phone-call';
import { createElapsed } from '../primitives/elapsed';
import type {
  PhoneCallSession,
  PhoneCallSessionState,
} from '../primitives/phone-call-session';

type ShownCall = Exclude<PhoneCallSessionState, { t: 'idle' }>;

/** The docked card for the phone call the viewer is on, if any. */
export function ActivePhoneCall(props: { session: PhoneCallSession }) {
  const shown = (): ShownCall | undefined => {
    const state = props.session.state();
    return state.t === 'idle' ? undefined : state;
  };
  // Keyed by call so keypad state and the clock start fresh for each call.
  const callId = () => shown()?.callId;
  return (
    <Show when={callId()} keyed>
      {(_callId) => (
        <Show when={shown()}>
          {(call) => <PhoneCallCard call={call()} session={props.session} />}
        </Show>
      )}
    </Show>
  );
}

function PhoneCallCard(props: { call: ShownCall; session: PhoneCallSession }) {
  const context = usePhoneContext();
  const [keypadOpen, setKeypadOpen] = createSignal(false);
  const [sentDigits, setSentDigits] = createSignal('');
  const elapsed = createElapsed(() =>
    props.call.t === 'live' ? props.call.activeSince : null
  );

  const leg = () => props.call.leg;
  const contactName = () => leg().contact?.name?.trim();
  const status = () => {
    if (props.call.t === 'ended') return props.call.outcome;
    const time = elapsed();
    if (time !== null) return formatCallDuration(time);
    return leg().direction === 'outbound' ? 'Calling…' : 'Connecting…';
  };

  async function press(key: string) {
    if (dtmfCode(key) === null) return;
    setSentDigits((digits) => (digits + key).slice(-24));
    try {
      await context.media.sendDigit(key);
    } catch {
      // A missed tone is no reason to interrupt the call; press again.
    }
  }

  return (
    <ActivePhoneCallCard
      title={remotePartyLabel(leg())}
      number={contactName() ? formatPhoneNumber(leg().remoteNumber) : undefined}
      status={status()}
      ended={props.call.t === 'ended'}
      muted={context.media.isMuted()}
      keypadOpen={keypadOpen()}
      sentDigits={sentDigits()}
      onToggleMute={() => void context.media.toggleMute()}
      onToggleKeypad={() => setKeypadOpen((open) => !open)}
      onKey={(key) => void press(key)}
      onHangUp={() => void props.session.hangUp()}
      onOpenContact={
        leg().contact
          ? () => {
              const contact = leg().contact;
              if (contact) context.openContact(contact.contactId);
            }
          : undefined
      }
    />
  );
}
