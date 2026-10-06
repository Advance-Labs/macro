import { describe, expect, it } from 'vitest';
import { parsePhoneCallEvent } from './phone-events';

const leg = {
  direction: 'inbound',
  remoteNumber: '+15552345678',
  localNumber: '+15559870000',
  participantIdentity: 'sip_+15552345678',
  status: 'active',
  contact: { contactId: 'contact-1', name: 'Ada Lovelace' },
  answeredAt: '2026-10-06T12:00:05Z',
  endedAt: null,
};

describe('parsePhoneCallEvent', () => {
  it('reads a ringing call from a JSON string frame', () => {
    const event = parsePhoneCallEvent(
      'phone_call_incoming',
      JSON.stringify({
        callId: 'call-1',
        from: '+15552345678',
        to: '+15559870000',
        contact: { contactId: 'contact-1', name: null },
        startedAt: '2026-10-06T12:00:00Z',
      })
    );
    expect(event).toEqual({
      type: 'incoming',
      call: {
        callId: 'call-1',
        from: '+15552345678',
        to: '+15559870000',
        contact: { contactId: 'contact-1', name: null },
        startedAt: '2026-10-06T12:00:00Z',
      },
    });
  });

  it('reads a leg update from an object frame', () => {
    const event = parsePhoneCallEvent('phone_call_updated', {
      callId: 'call-1',
      phone: leg,
    });
    expect(event).toEqual({
      type: 'updated',
      callId: 'call-1',
      leg: {
        direction: 'inbound',
        status: 'active',
        remoteNumber: '+15552345678',
        localNumber: '+15559870000',
        participantIdentity: 'sip_+15552345678',
        contact: { contactId: 'contact-1', name: 'Ada Lovelace' },
        answeredAt: '2026-10-06T12:00:05Z',
        endedAt: null,
      },
    });
  });

  it('ignores other frames and malformed payloads', () => {
    expect(parsePhoneCallEvent('call_started', { callId: 'x' })).toBeNull();
    expect(parsePhoneCallEvent('phone_call_incoming', '{not json')).toBeNull();
    expect(
      parsePhoneCallEvent('phone_call_incoming', { callId: 'call-1' })
    ).toBeNull();
    expect(
      parsePhoneCallEvent('phone_call_updated', {
        callId: 'call-1',
        phone: { ...leg, status: 'teleported' },
      })
    ).toBeNull();
    expect(
      parsePhoneCallEvent('phone_call_updated', { phone: leg })
    ).toBeNull();
  });
});
