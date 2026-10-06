import { describe, expect, it } from 'vitest';
import {
  canToggleAddon,
  formatMonthlyPrice,
  minutesUsage,
  type PhonePlan,
  type PhonePlanSeat,
  seatName,
  seatPhoneStatus,
} from './phone-plan';

function seat(overrides: Partial<PhonePlanSeat> = {}): PhonePlanSeat {
  return {
    userId: 'macro|ada@example.com',
    tier: 'premium',
    phoneEnabled: false,
    phoneIncluded: false,
    addon: false,
    addonEndsAt: null,
    ...overrides,
  };
}

function plan(overrides: Partial<PhonePlan> = {}): PhonePlan {
  return {
    canManage: true,
    available: true,
    monthlyPriceCents: 1_500,
    seats: [],
    minutes: null,
    blockedMessage: null,
    ...overrides,
  };
}

describe('phone plan', () => {
  it('formats prices and names', () => {
    expect(formatMonthlyPrice(1_500)).toBe('$15/mo');
    expect(formatMonthlyPrice(1_250)).toBe('$12.50/mo');
    expect(seatName('macro|ada@example.com')).toBe('ada@example.com');
    expect(minutesUsage({ included: 1_000, used: 120 })).toBe(
      '120 of 1,000 minutes used'
    );
  });

  it('says where each seat gets phone calling from', () => {
    expect(
      seatPhoneStatus(seat({ phoneIncluded: true, tier: 'max' }), plan())
    ).toBe('Included with your plan');
    expect(seatPhoneStatus(seat({ addon: true }), plan())).toBe(
      'Phone add-on, $15/mo'
    );
    expect(seatPhoneStatus(seat(), plan())).toBe('Add Phone for $15/mo');
    expect(seatPhoneStatus(seat(), plan({ available: false }))).toBe(
      'The Phone add-on is coming soon'
    );
    expect(seatPhoneStatus(seat({ tier: 'free' }), plan())).toBe(
      'Needs a paid plan'
    );
    expect(
      seatPhoneStatus(seat({ addonEndsAt: '2026-11-01T12:00:00Z' }), plan())
    ).toMatch(/^Add-on turned off; calling ends /);
  });

  it('lets only the payer change the add-on on Premium seats', () => {
    expect(canToggleAddon(seat(), plan())).toBe(true);
    expect(canToggleAddon(seat(), plan({ canManage: false }))).toBe(false);
    expect(canToggleAddon(seat(), plan({ available: false }))).toBe(false);
    expect(
      canToggleAddon(seat({ tier: 'max', phoneIncluded: true }), plan())
    ).toBe(false);
    expect(canToggleAddon(seat({ tier: 'free' }), plan())).toBe(false);
  });
});
