import { createSignal, onCleanup } from 'solid-js';
import type { PhoneAlerts } from '../context/phone-context';
import type { IncomingPhoneCall, PhoneLeg } from '../core/phone-call';

/**
 * Longest a call rings here without word from the server. Callers' carriers
 * give up well before this; it only bounds a ring whose ending was missed.
 */
export const MAX_PHONE_RING_MS = 90_000;
const MAX_REMEMBERED_CALLS = 100;

type RingingResources = {
  stopRing: () => void;
  closeNotification?: () => void;
  timeout: ReturnType<typeof setTimeout>;
};

/**
 * Phone calls ringing for the viewer, with the ringtone and system
 * notification that last exactly as long as each call rings here.
 */
export function createIncomingPhoneCalls(deps: {
  alerts: PhoneAlerts;
  /** Answer from a system notification. */
  answer: (call: IncomingPhoneCall) => void;
  maxRingMs?: number;
}) {
  const maxRingMs = deps.maxRingMs ?? MAX_PHONE_RING_MS;
  const [calls, setCalls] = createSignal<IncomingPhoneCall[]>([]);
  const resources = new Map<string, RingingResources>();
  // Calls that stopped ringing, so a late or replayed event can't revive one.
  const finished = new Set<string>();

  const isRinging = (callId: string) =>
    calls().some((call) => call.callId === callId);

  function release(callId: string) {
    const resource = resources.get(callId);
    resources.delete(callId);
    if (!resource) return;
    clearTimeout(resource.timeout);
    resource.stopRing();
    resource.closeNotification?.();
  }

  /** Stop ringing for a call that was answered, declined, or ended. */
  function resolve(callId: string) {
    finished.add(callId);
    if (finished.size > MAX_REMEMBERED_CALLS) {
      const oldest = finished.values().next().value;
      if (oldest !== undefined) finished.delete(oldest);
    }
    setCalls((current) => current.filter((call) => call.callId !== callId));
    release(callId);
  }

  async function notify(call: IncomingPhoneCall) {
    try {
      const close = await deps.alerts.notify(call, {
        answer: () => deps.answer(call),
      });
      const resource = resources.get(call.callId);
      if (resource) resource.closeNotification = close;
      else close?.();
    } catch (error) {
      // The in-app card and ringtone remain if the system notification fails.
      console.warn('Could not show phone call notification', error);
    }
  }

  /** Start ringing for a call to one of the viewer's numbers. */
  function receive(call: IncomingPhoneCall) {
    if (finished.has(call.callId) || isRinging(call.callId)) return;
    setCalls((current) => [call, ...current]);
    resources.set(call.callId, {
      stopRing: deps.alerts.ring(
        `phone-${call.callId}`,
        () => !isRinging(call.callId),
        maxRingMs
      ),
      timeout: setTimeout(() => resolve(call.callId), maxRingMs),
    });
    void notify(call);
  }

  /** Apply a phone leg update: anything but ringing ends the ring. */
  function update(callId: string, leg: PhoneLeg) {
    if (leg.status !== 'ringing') resolve(callId);
  }

  onCleanup(() => {
    for (const callId of [...resources.keys()]) release(callId);
    setCalls([]);
  });

  return { calls, receive, resolve, update };
}

export type IncomingPhoneCalls = ReturnType<typeof createIncomingPhoneCalls>;
