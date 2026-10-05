import { fireEvent, render, screen } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { describe, expect, it } from 'vitest';
import { ChangesToggleButton } from './ChangesToggleButton';

describe('ChangesToggleButton', () => {
  it('compacts only the Changes label at narrow header widths', () => {
    render(() => <ChangesToggleButton open={false} onToggle={() => {}} />);
    const button = screen.getByRole('button', { name: 'Changes' });
    expect(
      screen
        .getByText('Changes')
        .classList.contains('@max-[28rem]/split-header:hidden')
    ).toBe(true);
    for (const utility of [
      '@max-[28rem]/split-header:w-7',
      '@max-[28rem]/split-header:px-0',
      '@max-[28rem]/split-header:gap-0',
      'shrink-0',
      'touch:min-w-9',
    ]) {
      expect(button.classList.contains(utility)).toBe(true);
    }
    expect(button.getAttribute('aria-pressed')).toBe('false');
    expect(button.textContent).toBe('Changes');
  });

  it('uses sidepanel-like accent emphasis with a background while open', () => {
    const [open, setOpen] = createSignal(false);
    render(() => (
      <ChangesToggleButton
        open={open()}
        onToggle={() => setOpen((value) => !value)}
      />
    ));
    const button = screen.getByRole('button', { name: 'Changes' });
    expect(button.getAttribute('data-variant')).toBe('ghost');
    fireEvent.click(button);
    expect(screen.getByRole('button', { name: 'Changes' })).toBe(button);
    expect(button.getAttribute('aria-pressed')).toBe('true');
    expect(button.getAttribute('data-variant')).toBe('accent');
    expect(button.classList.contains('bg-accent-bg')).toBe(true);
    expect(button.classList.contains('text-accent')).toBe(true);
    expect(
      button.classList.contains('aria-pressed:[--color-active:transparent]')
    ).toBe(true);
    expect(button.classList.contains('border-edge-button')).toBe(false);
    expect(button.textContent).toBe('Changes');
    fireEvent.click(button);
    expect(button.getAttribute('aria-pressed')).toBe('false');
    expect(button.getAttribute('data-variant')).toBe('ghost');
    expect(button.classList.contains('bg-accent-bg')).toBe(false);
    expect(button.classList.contains('text-accent')).toBe(false);
  });

  it('keeps its accessible name when the visible label hides', () => {
    render(() => <ChangesToggleButton open={true} onToggle={() => {}} />);
    const button = screen.getByRole('button', { name: 'Changes' });
    screen.getByText('Changes').style.display = 'none';
    expect(screen.getByRole('button', { name: 'Changes' })).toBe(button);
    expect(button.getAttribute('aria-pressed')).toBe('true');
  });
});
