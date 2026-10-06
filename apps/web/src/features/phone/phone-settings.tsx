import { useUserId } from '@core/context/user';
import { openPhoneDialer } from './phone-actions';
import { usePhoneSettingsSource } from './queries/phone-calls';
import { usePhonePlanSource } from './queries/phone-plan';
import { PhoneSettingsView } from './views/phone-settings';

/** The Phone settings tab. */
export function PhoneSettings() {
  return (
    <PhoneSettingsView
      settings={usePhoneSettingsSource(useUserId())}
      plan={usePhonePlanSource()}
      onOpenDialer={() => openPhoneDialer()}
    />
  );
}
