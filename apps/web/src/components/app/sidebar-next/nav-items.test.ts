import { describe, expect, it } from 'vitest';
import { visibleNavItems } from './nav-items';

describe('reminders navigation item', () => {
  it('exposes the canonical list destination only while enabled', () => {
    const enabled = visibleNavItems({
      showCalendar: true,
      showCustomers: true,
      showReminders: true,
    });
    const disabled = visibleNavItems({
      showCalendar: true,
      showCustomers: true,
      showReminders: false,
    });

    expect(enabled.find((item) => item.id === 'reminders')).toMatchObject({
      label: 'Reminders',
      href: '/reminders',
    });
    expect(disabled.some((item) => item.id === 'reminders')).toBe(false);
  });
});

describe('Legal navigation item', () => {
  it('keeps the Legal workspace available independently of optional sections', () => {
    const items = visibleNavItems({
      showCalendar: false,
      showCustomers: false,
      showReminders: false,
    });
    expect(items.find((item) => item.id === 'legal')).toMatchObject({
      label: 'Legal',
      href: '/legal',
    });
  });
});
