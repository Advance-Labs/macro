import { ThrownResultError } from '@core/util/result';
import {
  useAiBillingSummaryQuery,
  usePhoneAddonQuery,
  useSetPhoneAddonMutation,
} from '@queries/auth';
import type {
  AiUsageSnapshot,
  PhoneAddonOverview,
} from '@service-auth/ai-billing-types';
import { createSignal } from 'solid-js';
import type { PhonePlanSource } from '../context/phone-context';
import type { PhonePlan } from '../core/phone-plan';

const BLOCKED_MESSAGES: Record<string, string> = {
  phone_plan_required: 'Phone calls need the Phone add-on or a Max plan.',
  phone_minutes_exhausted:
    "You've used this period's included minutes. Add credits or turn on usage billing in Usage to keep calling.",
  overage_limit_reached:
    "You've reached your usage billing limit for this period. Raise it in Usage to keep calling.",
  overage_payment_failed:
    'Your last usage charge did not go through. Update your payment method in Billing to keep calling.',
};

/** The plan as the Phone settings show it. */
export function toPhonePlan(
  overview: PhoneAddonOverview,
  summary: AiUsageSnapshot | undefined
): PhonePlan {
  const reason = summary?.phone_blocked_reason;
  return {
    canManage: overview.can_manage,
    available: overview.available,
    monthlyPriceCents: overview.monthly_price_cents,
    seats: overview.seats.map((seat) => ({
      userId: seat.user_id,
      tier: seat.tier,
      phoneEnabled: seat.phone_enabled,
      phoneIncluded: seat.phone_included,
      addon: seat.addon,
      addonEndsAt: seat.addon_ends_at ?? null,
    })),
    minutes:
      summary?.phone_enabled && !summary.unlimited
        ? {
            included: summary.phone_included_minutes ?? 0,
            used: summary.phone_used_minutes ?? 0,
          }
        : null,
    blockedMessage: reason ? (BLOCKED_MESSAGES[reason] ?? null) : null,
  };
}

/** The viewer's phone plan, read without suspending. */
export function usePhonePlanSource(): PhonePlanSource {
  const overview = usePhoneAddonQuery();
  const summary = useAiBillingSummaryQuery();
  const mutation = useSetPhoneAddonMutation();
  const [pendingSeat, setPendingSeat] = createSignal<string | null>(null);
  return {
    plan: () => {
      if (!overview.isSuccess) return undefined;
      return toPhonePlan(
        overview.data,
        summary.isSuccess ? summary.data : undefined
      );
    },
    isError: () => overview.isError,
    pendingSeat,
    setAddon: async (userId, enabled) => {
      setPendingSeat(userId);
      try {
        await mutation.mutateAsync({ userId, enabled });
      } catch (error) {
        const failure =
          error instanceof ThrownResultError ? error.errors[0] : undefined;
        throw new Error(
          failure?.message || 'Could not change the Phone add-on. Try again.'
        );
      } finally {
        setPendingSeat(null);
      }
    },
  };
}
