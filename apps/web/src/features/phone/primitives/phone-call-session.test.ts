import { createRoot } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PhoneCallJoin } from '../context/phone-context';
import type { PhoneLeg } from '../core/phone-call';
import { PhoneCallError } from '../core/phone-call-error';
import {
  createPhoneCallSession,
  ENDED_CALL_DISPLAY_MS,
} from './phone-call-session';

const leg = (overrides: Partial<PhoneLeg> = {}): PhoneLeg => ({
  direction: 'outbound',
  status: 'dialing',
  remoteNumber: '+15552345678',
  localNumber: '+15559870000',
  participantIdentity: 'sip_+15552345678',
  contact: null,
  answeredAt: null,
  endedAt: null,
  ...overrides,
});

const joined = (callId: string, phone: PhoneLeg): PhoneCallJoin => ({
  credentials: {
    callId,
    roomName: callId,
    serverUrl: 'wss://rtc.example',
    token: 'token',
    participantId: 'macro|me@example.com',
  },
  leg: phone,
});

const disposers: (() => void)[] = [];

function setup(options: { connectFails?: boolean } = {}) {
  return createRoot((dispose) => {
    disposers.push(dispose);
    let onDisconnected: (() => void) | undefined;
    const operations = {
      dial: vi.fn(async (_to: string) => joined('call-1', leg())),
      answer: vi.fn(async (callId: string) =>
        joined(callId, leg({ direction: 'inbound', status: 'active' }))
      ),
      hangUp: vi.fn(async (_callId: string) => undefined),
    };
    const media = {
      connect: vi.fn(
        async (_credentials: unknown, disconnected: () => void) => {
          if (options.connectFails) throw new Error('no microphone');
          onDisconnected = disconnected;
        }
      ),
      disconnect: vi.fn(async () => undefined),
    };
    const session = createPhoneCallSession({
      operations,
      media,
      now: () => Date.now(),
    });
    return {
      session,
      operations,
      media,
      roomClosed: () => onDisconnected?.(),
    };
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date('2026-10-06T12:00:00Z'));
});

afterEach(() => {
  for (const dispose of disposers.splice(0)) dispose();
  vi.useRealTimers();
});

describe('createPhoneCallSession', () => {
  it('joins a dialed call and starts the clock when it is answered', async () => {
    const { session, operations, media } = setup();

    await session.dial('(555) 234-5678');

    expect(operations.dial).toHaveBeenCalledWith('(555) 234-5678');
    expect(media.connect).toHaveBeenCalledOnce();
    expect(session.state()).toMatchObject({
      t: 'live',
      callId: 'call-1',
      activeSince: null,
    });

    vi.advanceTimersByTime(5_000);
    session.update('call-1', leg({ status: 'active' }));
    expect(session.state()).toMatchObject({
      t: 'live',
      activeSince: Date.now(),
    });
  });

  it('says why an outbound call did not connect, then clears', async () => {
    const { session, media } = setup();
    await session.dial('5552345678');

    session.update('call-1', leg({ status: 'busy' }));

    expect(session.state()).toMatchObject({ t: 'ended', outcome: 'Line busy' });
    expect(media.disconnect).toHaveBeenCalledOnce();
    vi.advanceTimersByTime(ENDED_CALL_DISPLAY_MS);
    expect(session.state()).toEqual({ t: 'idle' });
  });

  it('keeps the precise outcome when the room closes before the update', async () => {
    const { session, roomClosed } = setup();
    await session.dial('5552345678');

    roomClosed();
    expect(session.state()).toMatchObject({
      t: 'ended',
      outcome: 'Call ended',
    });

    session.update('call-1', leg({ status: 'no_answer' }));
    expect(session.state()).toMatchObject({ t: 'ended', outcome: 'No answer' });
  });

  it('ends the call for everyone on hang up', async () => {
    const { session, operations, media } = setup();
    await session.answer({
      callId: 'call-2',
      from: '+15552345678',
      to: null,
      contact: null,
      startedAt: new Date().toISOString(),
    });
    expect(session.state()).toMatchObject({
      t: 'live',
      callId: 'call-2',
      activeSince: Date.now(),
    });

    await session.hangUp();

    expect(operations.hangUp).toHaveBeenCalledWith('call-2');
    expect(media.disconnect).toHaveBeenCalledOnce();
    expect(session.state()).toMatchObject({
      t: 'ended',
      outcome: 'Call ended',
    });
  });

  it('ignores updates for other calls', async () => {
    const { session } = setup();
    await session.dial('5552345678');

    session.update('call-9', leg({ status: 'busy' }));

    expect(session.state()).toMatchObject({ t: 'live', callId: 'call-1' });
  });

  it('hangs up a call whose audio could not connect', async () => {
    const { session, operations } = setup({ connectFails: true });

    await expect(session.dial('5552345678')).rejects.toBeInstanceOf(
      PhoneCallError
    );

    expect(operations.hangUp).toHaveBeenCalledWith('call-1');
    expect(session.state()).toMatchObject({
      t: 'ended',
      outcome: 'Could not connect audio',
    });
    expect(session.starting()).toBe(false);
  });

  it('reports dial failures without changing state', async () => {
    const { session, operations } = setup();
    operations.dial.mockRejectedValueOnce(
      new PhoneCallError('invalid', 'Enter a phone number')
    );

    await expect(session.dial('')).rejects.toThrow('Enter a phone number');

    expect(session.state()).toEqual({ t: 'idle' });
    expect(session.starting()).toBe(false);
  });
});
