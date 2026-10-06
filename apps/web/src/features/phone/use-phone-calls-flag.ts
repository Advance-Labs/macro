import { useFeatureFlag } from '@app/lib/analytics/posthog';
import { ENABLE_CALLS, enablePhoneCalls } from '@core/constant/featureFlags';

/** Phone calling rides on calls being available at all. */
export function usePhoneCallsFlag() {
  const flag = useFeatureFlag(enablePhoneCalls);
  return () => ({ ...flag(), enabled: ENABLE_CALLS && flag().enabled });
}
