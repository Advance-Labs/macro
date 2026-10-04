import { fireEvent, render, screen, waitFor } from '@solidjs/testing-library';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { CopyFilePathButton } from './CopyFilePathButton';

afterEach(() => {
  vi.useRealTimers();
});

describe('CopyFilePathButton', () => {
  it('shows and announces success temporarily without changing its accessible name', async () => {
    const onCopy = vi.fn(async () => true);
    render(() => <CopyFilePathButton path="src/a.ts" onCopy={onCopy} />);
    const button = screen.getByRole('button', { name: 'Copy path' });
    const initialIcon = button.querySelector('svg');
    fireEvent.click(button);
    await waitFor(() => expect(screen.getByText('Path copied')).toBeTruthy());
    expect(onCopy).toHaveBeenCalledWith('src/a.ts');
    expect(button.querySelector('svg')).not.toBe(initialIcon);
    expect(screen.getByRole('button', { name: 'Copy path' })).toBe(button);
    vi.useFakeTimers();
    // Start a fresh feedback window with fake timers.
    fireEvent.click(button);
    await Promise.resolve();
    vi.advanceTimersByTime(1800);
    expect(screen.queryByText('Path copied')).toBeNull();
  });

  it('does not show a check on failure', async () => {
    const onCopy = vi.fn(async () => false);
    render(() => <CopyFilePathButton path="src/a.ts" onCopy={onCopy} />);
    const button = screen.getByRole('button', { name: 'Copy path' });
    const initialIcon = button.querySelector('svg');
    fireEvent.click(button);
    await waitFor(() => expect(onCopy).toHaveBeenCalledOnce());
    expect(screen.queryByText('Path copied')).toBeNull();
    expect(button.querySelector('svg')).toBe(initialIcon);
  });

  it.each([
    [true, false],
    [false, true],
  ])(
    'keeps the latest result when overlapping copies finish out of order (%s then %s)',
    async (earlier, latest) => {
      vi.useFakeTimers();
      const resolve: ((copied: boolean) => void)[] = [];
      const onCopy = vi.fn(
        () => new Promise<boolean>((done) => resolve.push(done))
      );
      render(() => <CopyFilePathButton path="src/a.ts" onCopy={onCopy} />);
      const button = screen.getByRole('button', { name: 'Copy path' });
      fireEvent.click(button);
      fireEvent.click(button);
      resolve[1](latest);
      await Promise.resolve();
      expect(screen.queryByText('Path copied') !== null).toBe(latest);
      resolve[0](earlier);
      await Promise.resolve();
      expect(screen.queryByText('Path copied') !== null).toBe(latest);
      expect(vi.getTimerCount()).toBe(latest ? 1 : 0);
      vi.advanceTimersByTime(1800);
      expect(screen.queryByText('Path copied')).toBeNull();
    }
  );

  it('does not show an earlier success while the latest copy is pending', async () => {
    const resolve: ((copied: boolean) => void)[] = [];
    const onCopy = vi.fn(
      () => new Promise<boolean>((done) => resolve.push(done))
    );
    render(() => <CopyFilePathButton path="src/a.ts" onCopy={onCopy} />);
    const button = screen.getByRole('button', { name: 'Copy path' });
    fireEvent.click(button);
    fireEvent.click(button);
    resolve[0](true);
    await Promise.resolve();
    expect(screen.queryByText('Path copied')).toBeNull();
    resolve[1](true);
    await Promise.resolve();
    expect(screen.getByText('Path copied')).toBeTruthy();
  });

  it('clears its timer and ignores pending copies after unmount', async () => {
    vi.useFakeTimers();
    let resolve!: (result: boolean) => void;
    const onCopy = vi.fn(
      () =>
        new Promise<boolean>((done) => {
          resolve = done;
        })
    );
    const view = render(() => (
      <CopyFilePathButton path="src/a.ts" onCopy={onCopy} />
    ));
    fireEvent.click(screen.getByRole('button', { name: 'Copy path' }));
    resolve(true);
    await Promise.resolve();
    expect(vi.getTimerCount()).toBe(1);
    view.unmount();
    expect(vi.getTimerCount()).toBe(0);
    const pending = render(() => (
      <CopyFilePathButton path="src/b.ts" onCopy={onCopy} />
    ));
    fireEvent.click(screen.getByRole('button', { name: 'Copy path' }));
    pending.unmount();
    resolve(true);
    await Promise.resolve();
    expect(vi.getTimerCount()).toBe(0);
  });
});
