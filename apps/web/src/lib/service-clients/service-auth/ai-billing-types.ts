/**
 * Hand-written mirrors of the auth service's AI billing OpenAPI types
 * (`crates/ai_billing`). Replace with the orval-generated schemas on the next
 * client regeneration (`bun run gen-api auth-service`).
 *
 * Allowance and usage fields (`included_cents`, `used_cents`,
 * `remaining_cents`) are cents at provider cost. Credits, overage charges,
 * caps, packs and `uncovered_cents` are customer cents.
 */

export type AiPlanTier = 'free' | 'premium' | 'max';

export type AiDenyReason =
  | 'allowance_exhausted'
  | 'free_allowance_exhausted'
  | 'overage_limit_reached'
  | 'overage_payment_failed'
  | 'phone_plan_required'
  | 'phone_minutes_exhausted';

/** Machine-readable codes carried in 402 bodies from the AI endpoints. */
export type AiDenyCode =
  | 'ai_allowance_exhausted'
  | 'ai_free_allowance_exhausted'
  | 'ai_overage_limit_reached'
  | 'ai_overage_payment_failed';

export interface AiUsageSnapshot {
  tier: AiPlanTier;
  unlimited: boolean;
  payer: string;
  can_manage_billing: boolean;
  seats: number;
  period_start: string;
  period_end: string;
  included_cents: number;
  used_cents: number;
  credits_consumed_cents: number;
  credit_balance_cents: number;
  overage_enabled: boolean;
  overage_limit_cents: number;
  overage_charged_cents: number;
  overage_suspended: boolean;
  uncovered_cents: number;
  remaining_cents: number;
  blocked_reason?: AiDenyReason;
  /** Whether this seat can make phone calls. Absent from older servers. */
  phone_enabled?: boolean;
  /** Phone minutes included with this seat this period. */
  phone_included_minutes?: number;
  /** Phone minutes this seat has used this period. */
  phone_used_minutes?: number;
  /** Why phone calls are refused right now, if they are. */
  phone_blocked_reason?: AiDenyReason;
}

export interface AiPlanCatalogEntry {
  tier: AiPlanTier;
  monthly_price_cents: number;
  included_ai_cents_per_seat: number;
  purchasable: boolean;
  /** Whether every seat on this plan can make phone calls. */
  phone_included?: boolean;
}

export interface AiPlanCatalog {
  plans: AiPlanCatalogEntry[];
  credit_packs_cents: number[];
  overage_limit_min_cents: number;
  overage_limit_max_cents: number;
  /** Monthly price of the Phone add-on per Premium seat, cents. */
  phone_addon_monthly_price_cents?: number;
  /** Phone minutes included per phone seat per period. */
  included_phone_minutes_per_seat?: number;
}

/** One billed seat in the Phone add-on overview. */
export interface PhoneSeatStatus {
  user_id: string;
  tier: AiPlanTier;
  /** Whether the seat can make phone calls now. */
  phone_enabled: boolean;
  /** Whether calling comes with the seat's plan (Max or enterprise). */
  phone_included: boolean;
  /** Whether the seat's Phone add-on renews with the subscription. */
  addon: boolean;
  /** When a turned-off add-on stops. */
  addon_ends_at?: string;
}

/** The Phone add-on for the viewer's plan. */
export interface PhoneAddonOverview {
  /** Whether the viewer pays for the plan and may change the add-on. */
  can_manage: boolean;
  /** Whether the add-on can be bought in this deployment. */
  available: boolean;
  monthly_price_cents: number;
  included_minutes_per_seat: number;
  /** The viewer's own seat first, then (for the payer) every other seat. */
  seats: PhoneSeatStatus[];
}

export type PaidPlan = 'premium' | 'max';

/**
 * A team member with the plan their seat is billed at. Mirrors the auth
 * service's `TeamMember` once `plan` lands in the generated schema.
 */
export interface TeamMemberPlan {
  team_id: string;
  user_id: string;
  role: 'member' | 'admin' | 'owner';
  plan: PaidPlan;
}
