import { createSignal, onCleanup } from 'solid-js';
import type {
  PhoneCallJoin,
  PhoneCallOperations,
  PhoneMedia,
} from '../context/phone-context';
import {
  describePhoneCall,
  type IncomingPhoneCall,
  isLivePhoneStatus,
  type PhoneLeg,
} from '../core/phone-call';
import { PhoneCallError } from '../core/phone-call-error';

/** How long an ended call stays on screen saying how it ended. */
export const ENDED_CALL_DISPLAY_MS = 4_000;

export type PhoneCallSessionState =
  | { t: 'idle' }
  /**
   * The viewer is on the call. `activeSince` is the local time both sides
   * connected; `null` while it still dials or rings.
   */
  | { t: 'live'; callId: string; leg: PhoneLeg; activeSince: number | null }
  /** The call just ended; `outcome` says how. */
  | { t: 'ended'; callId: string; leg: PhoneLeg; outcome: string };

/** How a call that just ended is summarized for the person on it. */
function describeEnding(leg: PhoneLeg): string {
  return leg.status === 'completed' || isLivePhoneStatus(leg.status)
    ? 'Call ended'
    : describePhoneCall(leg);
}

/**
 * The viewer's one phone call at a time: placing or answering it, joining its
 * audio, following the phone leg, and hanging up.
 */
export function createPhoneCallSession(deps: {
  operations: Pick<PhoneCallOperations, 'dial' | 'answer' | 'hangUp'>;
  media: Pick<PhoneMedia, 'connect' | 'disconnect'>;
  now?: () => number;
}) {
  const now = deps.now ?? Date.now;
  const [state, setState] = createSignal<PhoneCallSessionState>({ t: 'idle' });
  const [starting, setStarting] = createSignal(false);
  let clearEnded: ReturnType<typeof setTimeout> | undefined;

  /** Best-effort cleanup whose failure changes nothing the viewer sees. */
  async function quietly(operation: () => Promise<unknown>) {
    try {
      await operation();
    } catch {
      // The server ends abandoned calls; the room closes with the call.
    }
  }

  function show(next: PhoneCallSessionState) {
    clearTimeout(clearEnded);
    setState(next);
  }

  function end(callId: string, leg: PhoneLeg, outcome: string) {
    const current = state();
    if (current.t !== 'live' || current.callId !== callId) return;
    show({ t: 'ended', callId, leg, outcome });
    clearEnded = setTimeout(() => {
      const latest = state();
      if (latest.t === 'ended' && latest.callId === callId)
        setState({ t: 'idle' });
    }, ENDED_CALL_DISPLAY_MS);
  }

  /** The room went away: the other party hung up, or the call was replaced. */
  function disconnected(callId: string) {
    const current = state();
    if (current.t === 'live' && current.callId === callId)
      end(callId, current.leg, describeEnding(current.leg));
  }

  async function join(start: () => Promise<PhoneCallJoin>) {
    if (starting())
      throw new PhoneCallError('failed', 'Another call is already starting.');
    setStarting(true);
    try {
      const { credentials, leg } = await start();
      const callId = credentials.callId;
      show({
        t: 'live',
        callId,
        leg,
        activeSince: leg.status === 'active' ? now() : null,
      });
      try {
        await deps.media.connect(credentials, () => disconnected(callId));
      } catch {
        // A call nobody can hear is no use to either side.
        void quietly(() => deps.operations.hangUp(callId));
        end(callId, leg, 'Could not connect audio');
        throw new PhoneCallError(
          'failed',
          'Could not connect your audio. Check your microphone and try again.'
        );
      }
    } finally {
      setStarting(false);
    }
  }

  /** Place a call to a number as typed. Rejects with a `PhoneCallError`. */
  function dial(to: string) {
    return join(() => deps.operations.dial(to));
  }

  /** Answer a ringing call. Rejects with a `PhoneCallError`. */
  function answer(call: IncomingPhoneCall) {
    return join(() => deps.operations.answer(call.callId));
  }

  /** End the call for everyone. */
  async function hangUp() {
    const current = state();
    if (current.t !== 'live') return;
    end(current.callId, current.leg, 'Call ended');
    await Promise.allSettled([
      deps.operations.hangUp(current.callId),
      deps.media.disconnect(),
    ]);
  }

  /** Apply a phone leg update pushed by the server. */
  function update(callId: string, leg: PhoneLeg) {
    const current = state();
    if (current.t === 'idle' || current.callId !== callId) return;
    if (current.t === 'ended') {
      // The room can close before the update saying why arrives.
      if (!isLivePhoneStatus(leg.status))
        setState({ ...current, leg, outcome: describeEnding(leg) });
      return;
    }
    if (isLivePhoneStatus(leg.status)) {
      setState({
        ...current,
        leg,
        activeSince:
          current.activeSince ?? (leg.status === 'active' ? now() : null),
      });
      return;
    }
    end(callId, leg, describeEnding(leg));
    void quietly(() => deps.media.disconnect());
  }

  onCleanup(() => clearTimeout(clearEnded));

  return { state, starting, dial, answer, hangUp, update };
}

export type PhoneCallSession = ReturnType<typeof createPhoneCallSession>;
