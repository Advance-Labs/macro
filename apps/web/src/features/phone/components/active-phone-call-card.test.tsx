import { cleanup, fireEvent, render, screen } from '@solidjs/testing-library';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ActivePhoneCallCard } from './active-phone-call-card';
import { Dialpad } from './dialpad';

afterEach(cleanup);

describe('Dialpad', () => {
  it('reports each key with its letters as the accessible name', () => {
    const onKey = vi.fn();
    render(() => <Dialpad onKey={onKey} />);

    fireEvent.click(screen.getByRole('button', { name: '2 ABC' }));
    fireEvent.click(screen.getByRole('button', { name: '#' }));
    fireEvent.click(screen.getByRole('button', { name: '0 +' }));

    expect(onKey.mock.calls).toEqual([['2'], ['#'], ['0']]);
  });
});

describe('ActivePhoneCallCard', () => {
  function renderCard(overrides: { ended?: boolean; keypadOpen?: boolean }) {
    const handlers = {
      onToggleMute: vi.fn(),
      onToggleKeypad: vi.fn(),
      onKey: vi.fn(),
      onHangUp: vi.fn(),
      onOpenContact: vi.fn(),
    };
    render(() => (
      <ActivePhoneCallCard
        title="Ada Lovelace"
        number="+1 (555) 234-5678"
        status={overrides.ended ? 'Line busy' : '1:05'}
        connected={!overrides.ended}
        ended={overrides.ended ?? false}
        muted={false}
        keypadOpen={overrides.keypadOpen ?? false}
        sentDigits="12#"
        {...handlers}
      />
    ));
    return handlers;
  }

  it('shows who the call is with and where it is', () => {
    const handlers = renderCard({});

    expect(screen.getByText('+1 (555) 234-5678')).toBeTruthy();
    expect(screen.getByRole('status').textContent).toBe('1:05');
    fireEvent.click(screen.getByRole('button', { name: 'Ada Lovelace' }));
    fireEvent.click(screen.getByRole('button', { name: 'Hang up' }));

    expect(handlers.onOpenContact).toHaveBeenCalledOnce();
    expect(handlers.onHangUp).toHaveBeenCalledOnce();
  });

  it('echoes sent tones while the keypad is open', () => {
    const handlers = renderCard({ keypadOpen: true });

    expect(screen.getByText('12#')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '5 JKL' }));

    expect(handlers.onKey).toHaveBeenCalledWith('5');
  });

  it('disables the controls once the call has ended', () => {
    renderCard({ ended: true, keypadOpen: true });

    expect(screen.getByRole('status').textContent).toBe('Line busy');
    expect(screen.queryByRole('group', { name: 'Keypad' })).toBeNull();
    expect(
      (screen.getByRole('button', { name: 'Hang up' }) as HTMLButtonElement)
        .disabled
    ).toBe(true);
  });
});
