import { cleanup, fireEvent, render, screen } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type {
  PhonePlanSource,
  PhoneSettingsSource,
} from '../context/phone-context';
import type { PhonePlan } from '../core/phone-plan';
import { PhoneSettingsView } from './phone-settings';

afterEach(cleanup);

const settings: PhoneSettingsSource = {
  settings: () => ({ dialingEnabled: true, callerId: null, phoneNumbers: [] }),
  isError: () => false,
};

function plan(overrides: Partial<PhonePlan> = {}): PhonePlan {
  return {
    canManage: true,
    available: true,
    monthlyPriceCents: 1_500,
    seats: [
      {
        userId: 'macro|owner@example.com',
        tier: 'premium',
        phoneEnabled: false,
        phoneIncluded: false,
        addon: false,
        addonEndsAt: null,
      },
      {
        userId: 'macro|max@example.com',
        tier: 'max',
        phoneEnabled: true,
        phoneIncluded: true,
        addon: false,
        addonEndsAt: null,
      },
    ],
    minutes: null,
    blockedMessage: null,
    ...overrides,
  };
}

function source(value: PhonePlan): PhonePlanSource & {
  setAddon: ReturnType<typeof vi.fn>;
} {
  const [pendingSeat] = createSignal<string | null>(null);
  return {
    plan: () => value,
    isError: () => false,
    pendingSeat,
    setAddon: vi.fn(async () => {}),
  };
}

describe('PhoneSettingsView plan', () => {
  it('lets the payer add Phone to a Premium seat', () => {
    const planSource = source(plan());
    render(() => (
      <PhoneSettingsView
        settings={settings}
        plan={planSource}
        onOpenDialer={() => {}}
      />
    ));

    expect(screen.getByText('Phone calling is off')).toBeTruthy();
    expect(screen.getByText('Add Phone for $15/mo')).toBeTruthy();
    expect(screen.getByText('Included with your plan')).toBeTruthy();
    fireEvent.click(
      screen.getByRole('switch', { name: 'Phone add-on for your seat' })
    );
    expect(planSource.setAddon).toHaveBeenCalledWith(
      'macro|owner@example.com',
      true
    );
    // Max seats have no add-on to change.
    expect(screen.getAllByRole('switch')).toHaveLength(1);
  });

  it('shows a member their minutes and nobody else’s seat', () => {
    render(() => (
      <PhoneSettingsView
        settings={settings}
        plan={source(
          plan({
            canManage: false,
            seats: [
              {
                userId: 'macro|member@example.com',
                tier: 'premium',
                phoneEnabled: true,
                phoneIncluded: false,
                addon: true,
                addonEndsAt: null,
              },
            ],
            minutes: { included: 1_000, used: 42 },
          })
        )}
        onOpenDialer={() => {}}
      />
    ));

    expect(screen.getByText('Phone calling is on')).toBeTruthy();
    expect(
      screen.getByText(
        '42 of 1,000 minutes used this period. Extra minutes are billed as usage.'
      )
    ).toBeTruthy();
    expect(screen.queryByRole('switch')).toBeNull();
  });
});
