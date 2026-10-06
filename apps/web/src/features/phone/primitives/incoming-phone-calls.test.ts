import { createRoot } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PhoneAlerts } from '../context/phone-context';
import type { IncomingPhoneCall, PhoneLeg } from '../core/phone-call';
import { createIncomingPhoneCalls } from './incoming-phone-calls';

const call: IncomingPhoneCall = {
  callId: 'call-1',
  from: '+15552345678',
  to: '+15559870000',
  contact: { contactId: 'contact-1', name: 'Ada Lovelace' },
  startedAt: '2026-10-06T12:00:00Z',
};

const leg = (status: PhoneLeg['status']): PhoneLeg => ({
  direction: 'inbound',
  status,
  remoteNumber: call.from,
  localNumber: call.to,
  participantIdentity: 'sip_+15552345678',
  contact: call.contact,
  answeredAt: null,
  endedAt: null,
});

const disposers: (() => void)[] = [];

function setup() {
  return createRoot((dispose) => {
    disposers.push(dispose);
    const stopRing = vi.fn();
    const closeNotification = vi.fn();
    const alerts = {
      ring: vi.fn<PhoneAlerts['ring']>(() => stopRing),
      notify: vi.fn<PhoneAlerts['notify']>(async () => closeNotification),
    };
    const answer = vi.fn();
    const incoming = createIncomingPhoneCalls({
      alerts,
      answer,
      maxRingMs: 60_000,
    });
    return { incoming, alerts, answer, stopRing, closeNotification, dispose };
  });
}

async function settle() {
  await Promise.resolve();
  await Promise.resolve();
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  for (const dispose of disposers.splice(0)) dispose();
  vi.useRealTimers();
});

describe('createIncomingPhoneCalls', () => {
  it('rings and notifies once per call', async () => {
    const { incoming, alerts } = setup();

    incoming.receive(call);
    incoming.receive(call);
    await settle();

    expect(incoming.calls()).toEqual([call]);
    expect(alerts.ring).toHaveBeenCalledOnce();
    expect(alerts.notify).toHaveBeenCalledOnce();
  });

  it('answers from the system notification', async () => {
    const { incoming, alerts, answer } = setup();
    incoming.receive(call);
    await settle();

    alerts.notify.mock.calls[0]?.[1].answer();

    expect(answer).toHaveBeenCalledWith(call);
  });

  it('stops ringing when the call is answered elsewhere or missed', async () => {
    const { incoming, stopRing, closeNotification } = setup();
    incoming.receive(call);
    await settle();

    incoming.update('call-1', leg('ringing'));
    expect(incoming.calls()).toHaveLength(1);

    incoming.update('call-1', leg('missed'));
    expect(incoming.calls()).toEqual([]);
    expect(stopRing).toHaveBeenCalledOnce();
    expect(closeNotification).toHaveBeenCalledOnce();
  });

  it('does not revive a call that stopped ringing', async () => {
    const { incoming, alerts } = setup();
    incoming.receive(call);
    incoming.resolve('call-1');

    incoming.receive(call);

    expect(incoming.calls()).toEqual([]);
    expect(alerts.ring).toHaveBeenCalledOnce();
  });

  it('gives up ringing after the safety timeout', async () => {
    const { incoming, stopRing } = setup();
    incoming.receive(call);

    vi.advanceTimersByTime(60_000);

    expect(incoming.calls()).toEqual([]);
    expect(stopRing).toHaveBeenCalledOnce();
  });

  it('closes a notification that resolves after the call stopped ringing', async () => {
    const { incoming, closeNotification } = setup();
    incoming.receive(call);
    incoming.resolve('call-1');
    await settle();

    expect(closeNotification).toHaveBeenCalledOnce();
  });

  it('releases ringing resources when disposed', async () => {
    const { incoming, stopRing, dispose } = setup();
    incoming.receive(call);

    dispose();

    expect(stopRing).toHaveBeenCalledOnce();
  });
});
