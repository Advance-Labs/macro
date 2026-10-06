import { match } from 'ts-pattern';

/** Which side placed a phone call. */
export type PhoneCallDirection = 'outbound' | 'inbound';

/**
 * Where a phone leg is or how it ended. `dialing`, `ringing` and `active` are
 * live; the rest are outcomes and never change.
 */
export type PhoneCallStatus =
  | 'dialing'
  | 'ringing'
  | 'active'
  | 'completed'
  | 'missed'
  | 'no_answer'
  | 'busy'
  | 'declined'
  | 'failed'
  | 'cancelled';

/** The CRM contact matched to the other party. */
export type PhoneContact = {
  contactId: string;
  name: string | null;
};

/** The party on the phone network, as one call saw them. */
export type PhoneLeg = {
  direction: PhoneCallDirection;
  status: PhoneCallStatus;
  /** E.164. */
  remoteNumber: string;
  /** The Macro number used, in E.164, when known. */
  localNumber: string | null;
  /** RTC identity of the phone participant; also its transcript speaker id. */
  participantIdentity: string;
  contact: PhoneContact | null;
  answeredAt: string | null;
  endedAt: string | null;
};

/** A phone call ringing for the current user. */
export type IncomingPhoneCall = {
  callId: string;
  /** The caller's number, in E.164. */
  from: string;
  /** The Macro number they dialed, in E.164. */
  to: string | null;
  contact: PhoneContact | null;
  startedAt: string;
};

/** What the server tells a phone call's owner as the call progresses. */
export type PhoneCallEvent =
  /** A call to one of the viewer's numbers started ringing. */
  | { type: 'incoming'; call: IncomingPhoneCall }
  /** A phone leg was answered, or ended and how. */
  | { type: 'updated'; callId: string; leg: PhoneLeg };

const PHONE_CALL_STATUSES: readonly PhoneCallStatus[] = [
  'dialing',
  'ringing',
  'active',
  'completed',
  'missed',
  'no_answer',
  'busy',
  'declined',
  'failed',
  'cancelled',
];

/** Read a status in either spelling the API uses (`no_answer`, `NO_ANSWER`). */
export function toPhoneCallStatus(value: string): PhoneCallStatus | null {
  const status = value.toLowerCase();
  return PHONE_CALL_STATUSES.find((known) => known === status) ?? null;
}

/** Read a direction in either spelling the API uses. */
export function toPhoneCallDirection(value: string): PhoneCallDirection | null {
  const direction = value.toLowerCase();
  return direction === 'outbound' || direction === 'inbound' ? direction : null;
}

export function isLivePhoneStatus(status: PhoneCallStatus): boolean {
  return status === 'dialing' || status === 'ringing' || status === 'active';
}

const NORTH_AMERICAN_NUMBER = /^\+1(\d{3})(\d{3})(\d{4})$/;

/**
 * A readable phone number: `+1 (555) 234-5678` for North American numbers,
 * E.164 otherwise. Matches the server's rendering, so transcripts, summaries
 * and the UI agree.
 */
export function formatPhoneNumber(e164: string): string {
  const parts = NORTH_AMERICAN_NUMBER.exec(e164);
  if (!parts) return e164;
  const [, area, exchange, line] = parts;
  return `+1 (${area}) ${exchange}-${line}`;
}

/** How the other party is referred to: their contact name, else their number. */
export function remotePartyLabel(
  leg: Pick<PhoneLeg, 'contact' | 'remoteNumber'>
): string {
  return leg.contact?.name?.trim() || formatPhoneNumber(leg.remoteNumber);
}

/** One-line description of a phone call, as listed in Calls. */
export function describePhoneCall(
  leg: Pick<PhoneLeg, 'direction' | 'status'>
): string {
  return match(leg)
    .with({ status: 'dialing' }, () => 'Calling…')
    .with({ status: 'ringing' }, () => 'Incoming call')
    .with({ status: 'active' }, () => 'On the phone')
    .with({ status: 'missed' }, () => 'Missed call')
    .with({ status: 'no_answer' }, () => 'No answer')
    .with({ status: 'busy' }, () => 'Line busy')
    .with({ status: 'declined', direction: 'outbound' }, () => 'Call declined')
    .with({ status: 'declined', direction: 'inbound' }, () => 'Declined')
    .with({ status: 'failed' }, () => 'Call failed')
    .with({ status: 'cancelled' }, () => 'Cancelled')
    .with({ status: 'completed', direction: 'outbound' }, () => 'Outgoing call')
    .with({ status: 'completed', direction: 'inbound' }, () => 'Incoming call')
    .exhaustive();
}

/** Whether a finished call never connected, which lists it as a failure. */
export function isUnansweredPhoneStatus(status: PhoneCallStatus): boolean {
  return (
    status === 'missed' ||
    status === 'no_answer' ||
    status === 'busy' ||
    status === 'declined' ||
    status === 'failed'
  );
}

/** RFC 4733 DTMF event codes for the keypad keys. */
const DTMF_CODES: Record<string, number> = {
  '0': 0,
  '1': 1,
  '2': 2,
  '3': 3,
  '4': 4,
  '5': 5,
  '6': 6,
  '7': 7,
  '8': 8,
  '9': 9,
  '*': 10,
  '#': 11,
};

/** The DTMF event code for a keypad key, or `null` for other keys. */
export function dtmfCode(key: string): number | null {
  return DTMF_CODES[key] ?? null;
}

/** Keypad keys in display order, with the letters printed under each digit. */
export const KEYPAD_KEYS: readonly { key: string; letters: string }[] = [
  { key: '1', letters: '' },
  { key: '2', letters: 'ABC' },
  { key: '3', letters: 'DEF' },
  { key: '4', letters: 'GHI' },
  { key: '5', letters: 'JKL' },
  { key: '6', letters: 'MNO' },
  { key: '7', letters: 'PQRS' },
  { key: '8', letters: 'TUV' },
  { key: '9', letters: 'WXYZ' },
  { key: '*', letters: '' },
  { key: '0', letters: '+' },
  { key: '#', letters: '' },
];

/**
 * Whether typed text could be a phone number worth sending to the server,
 * which does the real parsing (extensions, `tel:` URIs, national formats).
 */
export function looksDialable(input: string): boolean {
  const number = input.split(/ext\.?|extension|[x#,;]/i)[0] ?? '';
  const digits = number.replace(/\D/g, '');
  return digits.length >= 7 && digits.length <= 15;
}

/** `m:ss` or `h:mm:ss` for an elapsed call time. */
export function formatCallDuration(elapsedMs: number): string {
  const totalSeconds = Math.max(0, Math.floor(elapsedMs / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = String(totalSeconds % 60).padStart(2, '0');
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, '0')}:${seconds}`
    : `${minutes}:${seconds}`;
}
