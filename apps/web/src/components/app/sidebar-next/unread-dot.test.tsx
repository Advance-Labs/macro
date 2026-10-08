import { cleanup, render } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { afterEach, describe, expect, it } from 'vitest';
import { SidebarUnreadDot } from './unread-dot';

afterEach(cleanup);

describe('SidebarUnreadDot', () => {
  it.each(['activity', 'important'] as const)(
    'renders a fixed-size, accent-colored %s circle',
    (kind) => {
      const { container } = render(() => (
        <SidebarUnreadDot active kind={kind} />
      ));
      const dot = container.querySelector('[data-channel-notification-kind]');
      expect(dot?.classList.contains('block')).toBe(true);
      expect(dot?.classList.contains('size-2.5')).toBe(true);
      expect(dot?.classList.contains('size-2')).toBe(false);
      expect(dot?.classList.contains('rounded-full')).toBe(true);
      expect(dot?.classList.contains('bg-ink')).toBe(false);
      expect(dot?.classList.contains('border-ink')).toBe(false);
      if (kind === 'activity') {
        expect(dot?.classList.contains('border-accent')).toBe(true);
        expect(dot?.classList.contains('bg-ink/10')).toBe(true);
        expect(dot?.classList.contains('bg-transparent')).toBe(false);
      } else {
        expect(dot?.classList.contains('bg-accent')).toBe(true);
        expect(dot?.classList.contains('bg-ink/10')).toBe(false);
        expect(dot?.classList.contains('border-2')).toBe(false);
      }
      expect(
        container
          .querySelector('[data-sidebar-unread-dot]')
          ?.classList.contains('opacity-100')
      ).toBe(true);
    }
  );

  it('keeps the accent color when changing from hollow to filled', () => {
    const [important, setImportant] = createSignal(false);
    const { container } = render(() => (
      <SidebarUnreadDot active kind={important() ? 'important' : 'activity'} />
    ));
    expect(
      container
        .querySelector('[data-channel-notification-kind]')
        ?.classList.contains('border-accent')
    ).toBe(true);
    setImportant(true);
    const dot = container.querySelector('[data-channel-notification-kind]');
    expect(dot?.classList.contains('size-2.5')).toBe(true);
    expect(dot?.classList.contains('bg-accent')).toBe(true);
    expect(dot?.classList.contains('border-accent')).toBe(false);
  });
});
