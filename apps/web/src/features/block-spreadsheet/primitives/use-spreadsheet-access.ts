import { useFeatureFlag } from '@app/lib/analytics/posthog';
import { enableSpreadsheets } from '@core/constant/featureFlags';

/** PostHog owns rollout targeting, just like other app feature flags. */
export function useSpreadsheetAccess() {
  const flag = useFeatureFlag(enableSpreadsheets);
  return () => flag().enabled;
}

/** Whether this session's rollout answer is still pending. */
export function useSpreadsheetAccessLoading() {
  const flag = useFeatureFlag(enableSpreadsheets);
  return () => flag().loading;
}
