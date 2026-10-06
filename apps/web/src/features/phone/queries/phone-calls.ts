import { ThrownResultError } from '@core/util/result';
import {
  answerPhoneCall,
  dialPhone,
  fetchIncomingPhoneCalls,
  hangUpPhoneCall,
  usePhoneSettingsQuery,
} from '@queries/call/phone';
import type { Accessor } from 'solid-js';
import type {
  PhoneCallJoin,
  PhoneCallOperations,
  PhoneSettingsSource,
} from '../context/phone-context';
import type { IncomingPhoneCall } from '../core/phone-call';
import { PhoneCallError } from '../core/phone-call-error';
import { decodeIncomingPhoneCall, decodePhoneLeg } from './phone-wire';

/** Translate a transport failure into the feature's error vocabulary. */
export function toPhoneCallError(error: unknown): PhoneCallError {
  if (error instanceof PhoneCallError) return error;
  const failure =
    error instanceof ThrownResultError ? error.errors[0] : undefined;
  switch (failure?.code) {
    case 'PHONE_INVALID':
      return new PhoneCallError('invalid', failure.message);
    case 'PHONE_UNAVAILABLE':
      return new PhoneCallError(
        'unavailable',
        failure.message || "Phone calling isn't set up for your workspace yet."
      );
    case 'NOT_FOUND':
    case 'CONFLICT':
      return new PhoneCallError('gone', 'This call has already ended.');
    case 'FORBIDDEN':
      return new PhoneCallError(
        'invalid',
        "You can't answer this call. It rang for someone else."
      );
    case 'NETWORK_ERROR':
      return new PhoneCallError(
        'failed',
        'Could not reach Macro. Check your connection and try again.'
      );
    default:
      return new PhoneCallError(
        'failed',
        'Something went wrong with the call. Please try again.'
      );
  }
}

type ApiPhoneCallJoin = Awaited<ReturnType<typeof dialPhone>>;

function toPhoneCallJoin(response: ApiPhoneCallJoin): PhoneCallJoin {
  const leg = decodePhoneLeg(response.phone);
  if (!leg) throw new PhoneCallError('failed', 'The call could not be read.');
  return {
    credentials: {
      callId: response.call.callId,
      roomName: response.call.roomName,
      serverUrl: response.call.serverUrl,
      token: response.call.token,
      participantId: response.call.participantId,
    },
    leg,
  };
}

async function translated<T>(operation: () => Promise<T>): Promise<T> {
  try {
    return await operation();
  } catch (error) {
    throw toPhoneCallError(error);
  }
}

/** Phone call operations backed by the call service. */
export function createPhoneCallOperations(): PhoneCallOperations {
  return {
    dial: (to) => translated(async () => toPhoneCallJoin(await dialPhone(to))),
    answer: (callId) =>
      translated(async () => toPhoneCallJoin(await answerPhoneCall(callId))),
    hangUp: (callId) =>
      translated(async () => {
        await hangUpPhoneCall(callId);
      }),
    listIncoming: () =>
      translated(async () =>
        (await fetchIncomingPhoneCalls()).flatMap(
          (call): IncomingPhoneCall[] => {
            const decoded = decodeIncomingPhoneCall(call);
            return decoded ? [decoded] : [];
          }
        )
      ),
  };
}

/** The viewer's phone settings, read without suspending. */
export function usePhoneSettingsSource(
  userId: Accessor<string | undefined>
): PhoneSettingsSource {
  const query = usePhoneSettingsQuery(userId);
  return {
    settings: () => {
      if (!query.isSuccess) return undefined;
      return {
        dialingEnabled: query.data.dialingEnabled,
        callerId: query.data.callerId ?? null,
        phoneNumbers: query.data.phoneNumbers,
      };
    },
    isError: () => query.isError,
  };
}
