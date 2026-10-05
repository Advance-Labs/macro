import { getCurrentWindow } from '@tauri-apps/api/window';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { enableDesktopWindowChrome } from './desktopWindowChrome';

const native = vi.hoisted(() => ({
  setTitleBarStyle: vi.fn(),
  title: vi.fn(),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: vi.fn(() => native),
}));

beforeEach(() => {
  vi.clearAllMocks();
  native.setTitleBarStyle.mockResolvedValue(undefined);
  native.title.mockResolvedValue('Macro');
});

describe('enableDesktopWindowChrome', () => {
  it.each(['ios', 'android', 'windows', 'linux'] as const)(
    'leaves %s windows unchanged',
    async (os) => {
      expect(await enableDesktopWindowChrome(os)).toEqual({ enabled: false });
      expect(getCurrentWindow).not.toHaveBeenCalled();
    }
  );

  it('activates the macOS shell after native overlay succeeds', async () => {
    expect(await enableDesktopWindowChrome('macos')).toEqual({
      enabled: true,
    });
    expect(native.setTitleBarStyle).toHaveBeenCalledWith('overlay');
  });

  it('retains the existing shell when an older binary rejects the permission', async () => {
    native.setTitleBarStyle.mockRejectedValue(new Error('not allowed'));

    expect(await enableDesktopWindowChrome('macos')).toEqual({
      enabled: false,
    });
    expect(native.title).not.toHaveBeenCalled();
  });

  it('keeps the recording identity visible after hiding the native title', async () => {
    native.title.mockResolvedValue('Macro — Recording 2c0fc969');

    expect(await enableDesktopWindowChrome('macos')).toEqual({
      enabled: true,
      recordingId: '2c0fc969',
    });
  });

  it('keeps native control clearance if reading the title fails', async () => {
    native.title.mockRejectedValue(new Error('title unavailable'));

    expect(await enableDesktopWindowChrome('macos')).toEqual({ enabled: true });
  });
});
