/**
 * The viewer's phone plan: whether their seat can call, its minutes, and the
 * Phone add-on for each seat they pay for.
 */

/** A seat's plan, as far as phone calling goes. */
export type PhonePlanTier = 'free' | 'premium' | 'max';

export type PhonePlanSeat = {
  userId: string;
  tier: PhonePlanTier;
  /** Whether the seat can make phone calls now. */
  phoneEnabled: boolean;
  /** Whether calling comes with the seat's plan (Max or enterprise). */
  phoneIncluded: boolean;
  /** Whether the seat's add-on renews. */
  addon: boolean;
  /** When a turned-off add-on stops, as an ISO timestamp. */
  addonEndsAt: string | null;
};

export type PhonePlan = {
  /** Whether the viewer pays for the plan and may change the add-on. */
  canManage: boolean;
  /** Whether the add-on can be bought in this deployment. */
  available: boolean;
  monthlyPriceCents: number;
  /** The viewer's own seat first, then every other seat they pay for. */
  seats: PhonePlanSeat[];
  /** The viewer's minutes this period, once their usage has loaded. */
  minutes: { included: number; used: number } | null;
  /** Why the viewer can't call right now, if they can't. */
  blockedMessage: string | null;
};

/** `$15/mo` for 1500 cents. */
export function formatMonthlyPrice(cents: number): string {
  const dollars = cents / 100;
  const amount = Number.isInteger(dollars)
    ? dollars.toString()
    : dollars.toFixed(2);
  return `$${amount}/mo`;
}

/** A user id as people read it: `macro|ada@example.com` → `ada@example.com`. */
export function seatName(userId: string): string {
  const separator = userId.indexOf('|');
  return separator === -1 ? userId : userId.slice(separator + 1);
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
  });
}

/** What a seat's phone calling comes from, in a few words. */
export function seatPhoneStatus(seat: PhonePlanSeat, plan: PhonePlan): string {
  const price = formatMonthlyPrice(plan.monthlyPriceCents);
  if (seat.phoneIncluded) return 'Included with your plan';
  if (seat.tier === 'free') return 'Needs a paid plan';
  if (seat.addon) return `Phone add-on, ${price}`;
  if (seat.addonEndsAt)
    return `Add-on turned off; calling ends ${formatDate(seat.addonEndsAt)}`;
  if (!plan.available) return 'The Phone add-on is coming soon';
  return `Add Phone for ${price}`;
}

/** Whether the viewer can turn the add-on on or off for `seat`. */
export function canToggleAddon(seat: PhonePlanSeat, plan: PhonePlan): boolean {
  return (
    plan.canManage &&
    plan.available &&
    seat.tier === 'premium' &&
    !seat.phoneIncluded
  );
}

/** `120 of 1,000 minutes used`. */
export function minutesUsage(minutes: {
  included: number;
  used: number;
}): string {
  const format = new Intl.NumberFormat();
  return `${format.format(minutes.used)} of ${format.format(minutes.included)} minutes used`;
}
