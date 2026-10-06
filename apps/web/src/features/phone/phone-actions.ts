import { useUserId } from '@core/context/user';
import { usePhoneSettingsQuery } from '@queries/call/phone';
import { type Accessor, createSignal } from 'solid-js';
import { usePhoneCallsFlag } from './use-phone-calls-flag';
import type { DialerRequest } from './views/dialer-dialog';

// One dialer for the app; `PhoneCallsProvider` shows it while a request is set.
const [dialerRequest, setDialerRequest] = createSignal<DialerRequest | null>(
  null
);

/** The dialer to show, if any. Read by the phone calls provider. */
export const phoneDialerRequest: Accessor<DialerRequest | null> = dialerRequest;

/** Open the dialer, optionally with a number filled in. */
export function openPhoneDialer(number = '') {
  setDialerRequest({ number, autoDial: false });
}

/** Call a number right away, showing the dialer for progress and errors. */
export function callPhoneNumber(number: string) {
  setDialerRequest({ number, autoDial: true });
}

export function closePhoneDialer() {
  setDialerRequest(null);
}

/** Whether the viewer can place phone calls: the rollout and their workspace. */
export function usePhoneDialingAvailable(): Accessor<boolean> {
  const flag = usePhoneCallsFlag();
  const enabled = () => !flag().loading && flag().enabled;
  const settings = usePhoneSettingsQuery(useUserId(), enabled);
  return () => enabled() && settings.isSuccess && settings.data.dialingEnabled;
}

/** Whether phone calling is rolled out to the viewer at all. */
export function usePhoneCallsEnabled(): Accessor<boolean> {
  const flag = usePhoneCallsFlag();
  return () => !flag().loading && flag().enabled;
}
