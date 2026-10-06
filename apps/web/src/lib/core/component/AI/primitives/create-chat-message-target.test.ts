import { createRoot, createSignal } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createChatMessageTarget } from './create-chat-message-target';

const disposers: Array<() => void> = [];
beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  disposers.splice(0).forEach((dispose) => dispose());
  document.body.replaceChildren();
  vi.useRealTimers();
});

function setup() {
  const container = document.createElement('div');
  container.dataset.chatScroll = '';
  document.body.append(container);
  const message = (id: string) => {
    const element = document.createElement('div');
    element.id = `chat-${id}`;
    element.scrollIntoView = vi.fn();
    container.append(element);
    return element;
  };
  const first = message('first');
  const second = message('second');
  const [params, setParams] = createSignal<Record<string, string> | undefined>(
    undefined,
    { equals: false }
  );
  const activeId = createRoot((dispose) => {
    disposers.push(dispose);
    return createChatMessageTarget({ params, container: () => container });
  });
  return { container, first, second, setParams, activeId };
}

describe('mounted chat message targeting', () => {
  it('only scrolls the latest request when requests arrive before the delayed scroll', () => {
    const target = setup();
    target.setParams({ message_id: 'first' });
    target.setParams({ message_id: 'second' });
    vi.advanceTimersByTime(0);
    expect(target.first.scrollIntoView).not.toHaveBeenCalled();
    expect(target.second.scrollIntoView).toHaveBeenCalledOnce();
    expect(target.activeId()).toBe('second');
  });

  it('repeats an identical target without an earlier timer clearing its new highlight', () => {
    const target = setup();
    const params = { message_id: 'first' };
    target.setParams(params);
    vi.advanceTimersByTime(1000);
    target.setParams(params);
    vi.advanceTimersByTime(500);
    expect(target.first.scrollIntoView).toHaveBeenCalledTimes(2);
    expect(target.activeId()).toBe('first');
    vi.advanceTimersByTime(1000);
    expect(target.activeId()).toBeUndefined();
  });

  it('cancels delayed work when the target clears or the transcript disposes', () => {
    const target = setup();
    target.setParams({ message_id: 'first' });
    target.setParams(undefined);
    vi.advanceTimersByTime(0);
    expect(target.activeId()).toBeUndefined();
    expect(target.first.scrollIntoView).not.toHaveBeenCalled();
    target.setParams({ message_id: 'second' });
    disposers.splice(0).forEach((dispose) => dispose());
    vi.runAllTimers();
    expect(target.second.scrollIntoView).not.toHaveBeenCalled();
  });

  it('never selects another transcript with the same message ID', () => {
    const other = document.createElement('div');
    other.id = 'chat-first';
    other.scrollIntoView = vi.fn();
    document.body.append(other);
    const target = setup();
    target.setParams({ message_id: 'first' });
    vi.advanceTimersByTime(0);
    expect(target.first.scrollIntoView).toHaveBeenCalledOnce();
    expect(other.scrollIntoView).not.toHaveBeenCalled();
  });
});
