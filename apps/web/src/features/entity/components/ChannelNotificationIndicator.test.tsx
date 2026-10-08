import { cleanup, render, screen } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { afterEach, describe, expect, it } from 'vitest';
import {
  ChannelNotificationIndicator,
  type ChannelNotificationKind,
} from './ChannelNotificationIndicator';

afterEach(cleanup);

describe('ChannelNotificationIndicator', () => {
  it('renders nothing for none', () => {
    const { container } = render(() => (
      <ChannelNotificationIndicator kind="none" />
    ));
    expect(container.childElementCount).toBe(0);
    expect(screen.queryByRole('img')).toBeNull();
  });

  it('renders an outlined dot with a low-opacity ink center for ordinary activity', () => {
    render(() => (
      <ChannelNotificationIndicator kind="activity" class="extra-class" />
    ));
    const dot = screen.getByRole('img', { name: 'Unread channel activity' });
    expect(dot.getAttribute('title')).toBe('Unread channel activity');
    for (const name of [
      'block',
      'size-2',
      'rounded-full',
      'border-2',
      'border-accent',
      'bg-ink/10',
      'extra-class',
    ]) {
      expect(dot.classList.contains(name)).toBe(true);
    }
    expect(dot.classList.contains('bg-accent')).toBe(false);
    expect(dot.classList.contains('bg-transparent')).toBe(false);
    expect(dot.classList.contains('border-ink')).toBe(false);
    expect(dot.textContent).toBe('');
  });

  it('renders a filled accent dot for mentions and thread replies', () => {
    render(() => <ChannelNotificationIndicator kind="important" />);
    const dot = screen.getByRole('img', {
      name: 'Unread mention or thread reply',
    });
    expect(dot.getAttribute('title')).toBe('Unread mention or thread reply');
    expect(dot.classList.contains('block')).toBe(true);
    expect(dot.classList.contains('size-2')).toBe(true);
    expect(dot.classList.contains('bg-accent')).toBe(true);
    expect(dot.classList.contains('bg-ink/10')).toBe(false);
    expect(dot.classList.contains('bg-ink')).toBe(false);
    expect(dot.classList.contains('border-2')).toBe(false);
    expect(dot.textContent).toBe('');
  });

  it('reacts to activity changes without changing dot size', () => {
    const [kind, setKind] = createSignal<ChannelNotificationKind>('none');
    const { container } = render(() => (
      <ChannelNotificationIndicator kind={kind()} />
    ));
    setKind('activity');
    expect(
      screen.getByRole('img', { name: 'Unread channel activity' })
    ).toBeTruthy();
    setKind('important');
    expect(
      screen.queryByRole('img', { name: 'Unread channel activity' })
    ).toBeNull();
    expect(
      screen
        .getByRole('img', { name: 'Unread mention or thread reply' })
        .classList.contains('size-2')
    ).toBe(true);
    setKind('none');
    expect(container.childElementCount).toBe(0);
  });
});
