import { cleanup, fireEvent, render } from '@solidjs/testing-library';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ThreadEarlierReplies } from '../ThreadEarlierReplies';

afterEach(cleanup);

describe('earlier reply disclosure', () => {
  it.each([
    [1, 'Show 1 earlier reply'],
    [3, 'Show 3 earlier replies'],
  ])('labels %i hidden replies', (count, label) => {
    const view = render(() => (
      <ThreadEarlierReplies count={count} onExpand={() => {}} />
    ));
    expect(view.getByRole('button', { name: label })).toBeTruthy();
  });

  it('requests expansion when clicked', () => {
    const onExpand = vi.fn();
    const view = render(() => (
      <ThreadEarlierReplies count={3} onExpand={onExpand} />
    ));
    const button = view.getByRole('button', { name: 'Show 3 earlier replies' });
    expect(button.getAttribute('aria-expanded')).toBe('false');
    fireEvent.click(button);
    expect(onExpand).toHaveBeenCalledOnce();
  });
});
