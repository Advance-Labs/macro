import { describe, expect, it } from 'vitest';
import {
  describePhoneCall,
  dtmfCode,
  formatCallDuration,
  formatPhoneNumber,
  isLivePhoneStatus,
  looksDialable,
  remotePartyLabel,
  toPhoneCallDirection,
  toPhoneCallStatus,
} from './phone-call';

describe('formatPhoneNumber', () => {
  it('groups North American numbers like the server does', () => {
    expect(formatPhoneNumber('+15552345678')).toBe('+1 (555) 234-5678');
  });

  it('leaves other numbers in E.164', () => {
    expect(formatPhoneNumber('+442079460958')).toBe('+442079460958');
    expect(formatPhoneNumber('+1555234567')).toBe('+1555234567');
  });
});

describe('remotePartyLabel', () => {
  it('prefers the contact name and falls back to the formatted number', () => {
    const number = '+15552345678';
    expect(
      remotePartyLabel({
        remoteNumber: number,
        contact: { contactId: 'c', name: 'Ada Lovelace' },
      })
    ).toBe('Ada Lovelace');
    expect(
      remotePartyLabel({
        remoteNumber: number,
        contact: { contactId: 'c', name: '  ' },
      })
    ).toBe('+1 (555) 234-5678');
    expect(remotePartyLabel({ remoteNumber: number, contact: null })).toBe(
      '+1 (555) 234-5678'
    );
  });
});

describe('statuses', () => {
  it('reads both API spellings', () => {
    expect(toPhoneCallStatus('no_answer')).toBe('no_answer');
    expect(toPhoneCallStatus('NO_ANSWER')).toBe('no_answer');
    expect(toPhoneCallStatus('ringing ')).toBeNull();
    expect(toPhoneCallDirection('INBOUND')).toBe('inbound');
    expect(toPhoneCallDirection('sideways')).toBeNull();
  });

  it('treats dialing, ringing, and active as live', () => {
    expect(isLivePhoneStatus('dialing')).toBe(true);
    expect(isLivePhoneStatus('ringing')).toBe(true);
    expect(isLivePhoneStatus('active')).toBe(true);
    expect(isLivePhoneStatus('completed')).toBe(false);
    expect(isLivePhoneStatus('busy')).toBe(false);
  });

  it('describes outcomes from the viewer’s side of the call', () => {
    expect(describePhoneCall({ direction: 'outbound', status: 'busy' })).toBe(
      'Line busy'
    );
    expect(describePhoneCall({ direction: 'inbound', status: 'missed' })).toBe(
      'Missed call'
    );
    expect(
      describePhoneCall({ direction: 'outbound', status: 'declined' })
    ).toBe('Call declined');
    expect(
      describePhoneCall({ direction: 'outbound', status: 'completed' })
    ).toBe('Outgoing call');
    expect(
      describePhoneCall({ direction: 'inbound', status: 'completed' })
    ).toBe('Incoming call');
  });
});

describe('dtmfCode', () => {
  it('maps keypad keys to RFC 4733 events', () => {
    expect(dtmfCode('0')).toBe(0);
    expect(dtmfCode('9')).toBe(9);
    expect(dtmfCode('*')).toBe(10);
    expect(dtmfCode('#')).toBe(11);
    expect(dtmfCode('A')).toBeNull();
    expect(dtmfCode('10')).toBeNull();
  });
});

describe('looksDialable', () => {
  it('accepts typed numbers worth sending to the server', () => {
    expect(looksDialable('(555) 234-5678')).toBe(true);
    expect(looksDialable('+44 20 7946 0958')).toBe(true);
    expect(looksDialable('555-234-5678 ext. 89')).toBe(true);
    expect(looksDialable('555 234 5678 x89')).toBe(true);
  });

  it('rejects too few or too many digits', () => {
    expect(looksDialable('')).toBe(false);
    expect(looksDialable('911')).toBe(false);
    expect(looksDialable('1234567890123456')).toBe(false);
  });
});

describe('formatCallDuration', () => {
  it('formats minutes and hours', () => {
    expect(formatCallDuration(0)).toBe('0:00');
    expect(formatCallDuration(65_400)).toBe('1:05');
    expect(formatCallDuration(3_725_000)).toBe('1:02:05');
    expect(formatCallDuration(-5)).toBe('0:00');
  });
});
