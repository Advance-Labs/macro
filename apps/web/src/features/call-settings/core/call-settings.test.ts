import { describe, expect, it } from 'vitest';
import {
  type CallSettings,
  isHuddleSharingBlocked,
  isRecordingBlocked,
  RECORDING_KINDS,
  type RecordingKinds,
  recordsByDefault,
  sharesHuddlesByDefault,
  withKind,
} from './call-settings';

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

const settings = (
  recordByDefault: RecordingKinds,
  recordingBlocked: RecordingKinds | null,
  sharing: { byDefault?: boolean; blocked?: boolean } = {}
): CallSettings => ({
  recordByDefault,
  shareHuddlesByDefault: sharing.byDefault ?? true,
  refuseOneOnOneRecording: false,
  team: recordingBlocked
    ? {
        recordingBlocked,
        huddleSharingBlocked: sharing.blocked ?? false,
        canEdit: false,
      }
    : null,
});

describe('call settings', () => {
  it('lists huddles, then meetings from smallest to most open', () => {
    expect(RECORDING_KINDS.map((option) => option.kind)).toEqual([
      'huddles',
      'oneOnOneMeetings',
      'internalMeetings',
      'externalMeetings',
    ]);
  });

  it('records the kinds a person chose when nothing is blocked', () => {
    const chosen = settings(withKind(NONE, 'huddles', true), null);
    expect(recordsByDefault(chosen, 'huddles')).toBe(true);
    expect(recordsByDefault(chosen, 'internalMeetings')).toBe(false);
    expect(isRecordingBlocked(chosen, 'huddles')).toBe(false);
  });

  it('never records a kind the team blocks', () => {
    const blocked = settings(ALL, withKind(NONE, 'externalMeetings', true));
    expect(isRecordingBlocked(blocked, 'externalMeetings')).toBe(true);
    expect(recordsByDefault(blocked, 'externalMeetings')).toBe(false);
    expect(recordsByDefault(blocked, 'internalMeetings')).toBe(true);
  });

  it('turns recording off when every default is cleared', () => {
    const off = settings(NONE, NONE);
    for (const option of RECORDING_KINDS) {
      expect(recordsByDefault(off, option.kind)).toBe(false);
    }
  });

  it('shares huddles only when chosen and the team allows it', () => {
    expect(sharesHuddlesByDefault(settings(ALL, null))).toBe(true);
    expect(
      sharesHuddlesByDefault(settings(ALL, null, { byDefault: false }))
    ).toBe(false);
    const blocked = settings(ALL, NONE, { blocked: true });
    expect(isHuddleSharingBlocked(blocked)).toBe(true);
    expect(sharesHuddlesByDefault(blocked)).toBe(false);
  });

  it('changes one kind without mutating the input', () => {
    const changed = withKind(ALL, 'internalMeetings', false);
    expect(changed).toEqual({ ...ALL, internalMeetings: false });
    expect(ALL.internalMeetings).toBe(true);
  });
});
