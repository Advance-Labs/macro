import type { PhoneCallEvent } from '../core/phone-call';
import { decodeIncomingPhoneCall, decodePhoneLeg } from './phone-wire';

function decodeFrame(raw: unknown): unknown {
  if (typeof raw !== 'string') return raw;
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

/** Read a websocket frame as a phone call event, or `null` for other frames. */
export function parsePhoneCallEvent(
  type: string,
  raw: unknown
): PhoneCallEvent | null {
  if (type !== 'phone_call_incoming' && type !== 'phone_call_updated')
    return null;
  const data = decodeFrame(raw);
  if (type === 'phone_call_incoming') {
    const call = decodeIncomingPhoneCall(data);
    return call ? { type: 'incoming', call } : null;
  }
  if (data === null || typeof data !== 'object' || Array.isArray(data))
    return null;
  const record = data as Record<string, unknown>;
  const callId = typeof record.callId === 'string' ? record.callId : null;
  const leg = decodePhoneLeg(record.phone);
  return callId && leg ? { type: 'updated', callId, leg } : null;
}
