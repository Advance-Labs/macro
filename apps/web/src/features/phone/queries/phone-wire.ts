import {
  type IncomingPhoneCall,
  type PhoneContact,
  type PhoneLeg,
  toPhoneCallDirection,
  toPhoneCallStatus,
} from '../core/phone-call';

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function text(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

/** Read a phone contact from the API or a websocket frame. */
export function decodePhoneContact(value: unknown): PhoneContact | null {
  if (!isRecord(value)) return null;
  const contactId = text(value.contactId);
  if (!contactId) return null;
  return { contactId, name: text(value.name) };
}

/**
 * Read a phone leg from the API or a websocket frame. Frames are untrusted
 * JSON, so a leg missing any required field reads as `null`.
 */
export function decodePhoneLeg(value: unknown): PhoneLeg | null {
  if (!isRecord(value)) return null;
  const direction =
    typeof value.direction === 'string'
      ? toPhoneCallDirection(value.direction)
      : null;
  const status =
    typeof value.status === 'string' ? toPhoneCallStatus(value.status) : null;
  const remoteNumber = text(value.remoteNumber);
  const participantIdentity = text(value.participantIdentity);
  if (!direction || !status || !remoteNumber || !participantIdentity)
    return null;
  return {
    direction,
    status,
    remoteNumber,
    localNumber: text(value.localNumber),
    participantIdentity,
    contact: decodePhoneContact(value.contact),
    answeredAt: text(value.answeredAt),
    endedAt: text(value.endedAt),
  };
}

/** Read a ringing phone call from the API or a websocket frame. */
export function decodeIncomingPhoneCall(
  value: unknown
): IncomingPhoneCall | null {
  if (!isRecord(value)) return null;
  const callId = text(value.callId);
  const from = text(value.from);
  const startedAt = text(value.startedAt);
  if (!callId || !from || !startedAt || !Number.isFinite(Date.parse(startedAt)))
    return null;
  return {
    callId,
    from,
    to: text(value.to),
    contact: decodePhoneContact(value.contact),
    startedAt,
  };
}
