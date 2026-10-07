import { cleanup, fireEvent, render, screen } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  type CallSettingsCapabilities,
  CallSettingsProvider,
} from '../context/call-settings-context';
import type {
  CallSettings,
  RecordingKinds,
  TeamCallPolicy,
} from '../core/call-settings';
import { CallSettingsView } from './call-settings-view';

afterEach(cleanup);

const ALL: RecordingKinds = {
  huddles: true,
  oneOnOneMeetings: true,
  internalMeetings: true,
  externalMeetings: true,
};
const NONE: RecordingKinds = {
  huddles: false,
  oneOnOneMeetings: false,
  internalMeetings: false,
  externalMeetings: false,
};

const settingsWith = (overrides: Partial<CallSettings> = {}): CallSettings => ({
  recordByDefault: ALL,
  shareHuddlesByDefault: true,
  refuseOneOnOneRecording: false,
  team: null,
  ...overrides,
});

const team = (overrides: Partial<TeamCallPolicy> = {}): TeamCallPolicy => ({
  recordingBlocked: NONE,
  huddleSharingBlocked: false,
  canEdit: true,
  ...overrides,
});

function renderView(
  initial: CallSettings | undefined,
  options: { error?: boolean } = {}
) {
  const [settings] = createSignal(initial);
  const capabilities: CallSettingsCapabilities = {
    createSource: () => ({
      settings,
      error: () => options.error ?? false,
    }),
    setRecordByDefault: vi.fn(),
    setShareHuddlesByDefault: vi.fn(),
    setRefuseOneOnOneRecording: vi.fn(),
    setRecordingBlocked: vi.fn(),
    setHuddleSharingBlocked: vi.fn(),
  };
  render(() => (
    <CallSettingsProvider value={capabilities}>
      <CallSettingsView />
    </CallSettingsProvider>
  ));
  return capabilities;
}

const checkbox = (name: string) =>
  screen.getByRole('checkbox', { name }) as HTMLInputElement;

describe('Calls settings', () => {
  it('shows each record-by-default option and saves a change', () => {
    const calls = renderView(
      settingsWith({
        recordByDefault: {
          ...ALL,
          oneOnOneMeetings: false,
          externalMeetings: false,
        },
      })
    );
    expect(checkbox('Huddles').checked).toBe(true);
    expect(checkbox('1:1 meetings').checked).toBe(false);
    expect(checkbox('Internal meetings').checked).toBe(true);
    expect(checkbox('External meetings').checked).toBe(false);

    fireEvent.click(checkbox('Huddles'));
    expect(calls.setRecordByDefault).toHaveBeenCalledWith('huddles', false);
    fireEvent.click(checkbox('1:1 meetings'));
    expect(calls.setRecordByDefault).toHaveBeenCalledWith(
      'oneOnOneMeetings',
      true
    );
  });

  it('places sharing between recording and 1:1 privacy', () => {
    renderView(settingsWith({ team: team() }));
    const headings = screen
      .getAllByText(
        /^(Record by default|Share by default|1:1 privacy|Team policy)$/
      )
      .map((heading) => heading.textContent);
    expect(headings).toEqual([
      'Record by default',
      'Share by default',
      '1:1 privacy',
      'Team policy',
    ]);
  });

  it('saves the huddle sharing default and says meetings are never shared', () => {
    const calls = renderView(settingsWith({ shareHuddlesByDefault: true }));
    const share = checkbox('Share huddles with my team');
    expect(share.checked).toBe(true);
    expect(screen.getByText(/Meetings are never shared/)).toBeTruthy();
    fireEvent.click(share);
    expect(calls.setShareHuddlesByDefault).toHaveBeenCalledWith(false);
  });

  it('locks huddle sharing off when the team blocks it', () => {
    const calls = renderView(
      settingsWith({
        team: team({ huddleSharingBlocked: true, canEdit: false }),
      })
    );
    const share = checkbox('Share huddles with my team');
    expect(share.checked).toBe(false);
    expect(share.disabled).toBe(true);
    expect(
      screen.getByText(
        'Your team admins have blocked sharing huddles with the team.'
      )
    ).toBeTruthy();
    fireEvent.click(share);
    expect(calls.setShareHuddlesByDefault).not.toHaveBeenCalled();
  });

  it('lets anyone refuse being recorded in 1:1s', () => {
    const calls = renderView(settingsWith({ team: team({ canEdit: false }) }));
    const refuse = checkbox("Don't record or transcribe my 1:1s");
    expect(refuse.checked).toBe(false);
    expect(refuse.disabled).toBe(false);
    fireEvent.click(refuse);
    expect(calls.setRefuseOneOnOneRecording).toHaveBeenCalledWith(true);
  });

  it('hides the team policy for people without a team', () => {
    renderView(settingsWith());
    expect(screen.queryByText('Team policy')).toBeNull();
  });

  it('lets team admins block recording and huddle sharing', () => {
    const calls = renderView(settingsWith({ team: team() }));
    expect(screen.queryByText('Admins only')).toBeNull();
    fireEvent.click(checkbox('Block recording internal meetings'));
    expect(calls.setRecordingBlocked).toHaveBeenCalledWith(
      'internalMeetings',
      true
    );
    fireEvent.click(checkbox('Block recording 1:1 meetings'));
    expect(calls.setRecordingBlocked).toHaveBeenCalledWith(
      'oneOnOneMeetings',
      true
    );
    fireEvent.click(checkbox('Block sharing huddles'));
    expect(calls.setHuddleSharingBlocked).toHaveBeenCalledWith(true);
  });

  it('shows the team policy to members but greys it out', () => {
    const calls = renderView(
      settingsWith({
        team: team({
          recordingBlocked: { ...NONE, huddles: true },
          canEdit: false,
        }),
      })
    );
    expect(screen.getByText('Admins only')).toBeTruthy();
    const names = [
      'Block recording huddles',
      'Block recording 1:1 meetings',
      'Block recording internal meetings',
      'Block recording external meetings',
      'Block sharing huddles',
    ];
    for (const name of names) {
      const block = checkbox(name);
      expect(block.disabled).toBe(true);
      expect(block.closest('[data-settings-target]')?.className).toContain(
        'cursor-not-allowed'
      );
      fireEvent.click(block);
    }
    expect(checkbox('Block recording huddles').checked).toBe(true);
    expect(calls.setRecordingBlocked).not.toHaveBeenCalled();
    expect(calls.setHuddleSharingBlocked).not.toHaveBeenCalled();
    expect(
      screen.getAllByText('Only team admins can change this.')
    ).toHaveLength(names.length);
  });

  it('turns off and locks a personal recording default the team blocks', () => {
    const calls = renderView(
      settingsWith({
        team: team({
          recordingBlocked: { ...NONE, externalMeetings: true },
          canEdit: false,
        }),
      })
    );
    const external = checkbox('External meetings');
    expect(external.checked).toBe(false);
    expect(external.disabled).toBe(true);
    expect(
      screen.getByText('Your team admins have blocked recording these calls.')
    ).toBeTruthy();
    fireEvent.click(external);
    expect(calls.setRecordByDefault).not.toHaveBeenCalled();
  });

  it('shows loading and error states', () => {
    renderView(undefined);
    expect(screen.getByRole('status').textContent).toContain('Loading');
    cleanup();
    renderView(undefined, { error: true });
    expect(screen.getByRole('alert').textContent).toContain("couldn't load");
  });
});
