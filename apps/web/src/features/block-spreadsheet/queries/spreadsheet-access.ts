import { analytics } from '@app/lib/analytics';
import {
  enableSpreadsheets,
  isFeatureEnabled,
} from '@core/constant/featureFlags';

/** Imperative rollout guard for loaders and creation, including keyboard entry points. */
export function isSpreadsheetEnabledForCurrentUser(): boolean {
  return isFeatureEnabled(enableSpreadsheets);
}

/**
 * Rollout decision for opening a workbook, such as one shared into a channel.
 *
 * PostHog answers from the flags it persisted on an earlier visit until this
 * session's `/flags` request lands, and that snapshot can predate the viewer
 * joining the rollout. Only a cached "on" is trusted immediately.
 */
export async function waitForSpreadsheetRollout(): Promise<boolean> {
  if (enableSpreadsheets.override !== undefined) {
    return enableSpreadsheets.override;
  }
  const { posthog } = analytics;
  const key = enableSpreadsheets.key;
  if (posthog.isFeatureEnabled(key)) return true;
  const fresh = posthog.isFeatureEnabled(key, { fresh: true });
  if (fresh !== undefined) return fresh;
  const cached = () => posthog.isFeatureEnabled(key) ?? false;

  return new Promise((resolve) => {
    let settled = false;
    let unsubscribe: (() => void) | undefined;
    const finish = (enabled: boolean) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      unsubscribe?.();
      resolve(enabled);
    };
    // A blocked or failed `/flags` request falls back to the persisted
    // snapshot instead of hanging navigation.
    const timeout = setTimeout(() => finish(cached()), 3_000);
    unsubscribe = posthog.onFeatureFlags((_flags, _variants, context) =>
      finish(
        context?.errorsLoading
          ? cached()
          : (posthog.isFeatureEnabled(key, { fresh: true }) ?? cached())
      )
    );
    // PostHog can invoke an already-ready callback before returning its cleanup.
    if (settled) unsubscribe();
  });
}
