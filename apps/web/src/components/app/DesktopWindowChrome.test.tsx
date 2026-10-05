import type { DesktopWindowChrome } from '@app/lib/tauri/desktopWindowChrome';
import { cleanup, render, screen } from '@solidjs/testing-library';
import { createSignal, type ParentProps } from 'solid-js';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DesktopWindowFrame, DesktopWindowHeader } from './DesktopWindowChrome';

const mocks = vi.hoisted(() => ({
  native: true,
  chrome: vi.fn<() => DesktopWindowChrome>(),
}));

vi.mock('@macro/tauri', () => ({
  useTauri: () =>
    mocks.native ? { desktopWindowChrome: mocks.chrome } : undefined,
}));

vi.mock('@ui/components/Tooltip', () => ({
  Tooltip: (props: ParentProps<{ label: string }>) => (
    <div title={props.label}>{props.children}</div>
  ),
}));

beforeEach(() => {
  mocks.native = true;
  mocks.chrome.mockReturnValue({ enabled: false });
});

afterEach(cleanup);

describe('desktop window chrome', () => {
  it('adds no header in browsers or when native activation is unavailable', () => {
    mocks.native = false;
    const browser = render(() => <DesktopWindowHeader />);
    expect(
      browser.container.querySelector('[data-tauri-drag-region]')
    ).toBeNull();
    browser.unmount();

    mocks.native = true;
    const unsupported = render(() => <DesktopWindowHeader />);
    expect(
      unsupported.container.querySelector('[data-tauri-drag-region]')
    ).toBeNull();
  });

  it('provides a drag region and recording identity on routes without a rail', () => {
    mocks.chrome.mockReturnValue({ enabled: true, recordingId: '2c0fc969' });
    const { container } = render(() => <DesktopWindowHeader />);

    expect(container.querySelector('[data-tauri-drag-region]')).not.toBeNull();
    expect(
      screen.getByRole('status', { name: 'Memory recording 2c0fc969' })
    ).toBeTruthy();
    expect(screen.getByTitle('Memory recording 2c0fc969')).toBeTruthy();
  });

  it('preserves route state when native activation completes after mounting', () => {
    const [chrome, setChrome] = createSignal<DesktopWindowChrome>({
      enabled: false,
    });
    mocks.chrome.mockImplementation(chrome);
    let mounts = 0;
    const Route = () => {
      mounts += 1;
      return <input aria-label="Meeting name" />;
    };
    const { container } = render(() => (
      <DesktopWindowFrame>
        <Route />
      </DesktopWindowFrame>
    ));
    const input = screen.getByRole('textbox') as HTMLInputElement;
    input.value = 'Planning';
    expect(container.querySelector('[data-tauri-drag-region]')).toBeNull();

    setChrome({ enabled: true });

    expect(container.querySelector('[data-tauri-drag-region]')).not.toBeNull();
    expect(screen.getByRole('textbox')).toBe(input);
    expect(input.value).toBe('Planning');
    expect(mounts).toBe(1);
  });
});
