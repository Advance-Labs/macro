import { getCurrentWindow } from '@tauri-apps/api/window';
import type { OsType } from '@tauri-apps/plugin-os';

export type DesktopWindowChrome = {
  enabled: boolean;
  recordingId?: string;
};

/** Older native binaries keep their title bar until they support the new shell. */
export async function enableDesktopWindowChrome(
  os: OsType
): Promise<DesktopWindowChrome> {
  if (os !== 'macos') return { enabled: false };

  const window = getCurrentWindow();
  try {
    await window.setTitleBarStyle('overlay');
  } catch {
    // OTA bundles can run on binaries without the window-style permission.
    return { enabled: false };
  }

  try {
    const title = await window.title();
    const recordingId = /^Macro — Recording ([0-9a-f]{8})$/i.exec(title)?.[1];
    return { enabled: true, recordingId };
  } catch {
    // A missing title must not leave the layout behind the native controls.
    return { enabled: true };
  }
}
