import { throwOnErr } from '@core/util/result';
import { callServiceClient } from '@service-call/client';
import { useQuery } from '@tanstack/solid-query';
import type { Accessor } from 'solid-js';
import { callKeys } from './keys';

/**
 * Whether the viewer can dial out, their caller id, and their numbers. Phone
 * settings change only when an operator assigns numbers, so they are fetched
 * once per session and refreshed in the background.
 */
export function usePhoneSettingsQuery(
  userId: Accessor<string | undefined>,
  enabled: Accessor<boolean> = () => true
) {
  return useQuery(() => ({
    queryKey: callKeys.phoneSettings(userId() ?? '').queryKey,
    queryFn: () => throwOnErr(() => callServiceClient.getPhoneSettings()),
    enabled: Boolean(userId()) && enabled(),
    staleTime: 5 * 60_000,
    retry: false,
  }));
}

/** Place a phone call; join the returned call to hear it ring. */
export function dialPhone(to: string) {
  return throwOnErr(() => callServiceClient.dialPhone(to));
}

/** Answer a phone call ringing for the viewer. */
export function answerPhoneCall(callId: string) {
  return throwOnErr(() => callServiceClient.answerPhoneCall(callId));
}

/** End a phone call for everyone, or decline it while it rings. */
export function hangUpPhoneCall(callId: string) {
  return throwOnErr(() => callServiceClient.hangUpPhoneCall(callId));
}

/** Phone calls ringing for the viewer right now. */
export async function fetchIncomingPhoneCalls() {
  const response = await throwOnErr(() =>
    callServiceClient.getIncomingPhoneCalls()
  );
  return response.calls;
}
