/** The kinds of call the recording settings tell apart. */
export type RecordingKind =
  | 'huddles'
  | 'oneOnOneMeetings'
  | 'internalMeetings'
  | 'externalMeetings';

/** One flag per {@link RecordingKind}. */
export type RecordingKinds = Record<RecordingKind, boolean>;

export type TeamCallPolicy = {
  /** Kinds no one on the team may record. */
  recordingBlocked: RecordingKinds;
  /** No one's huddles may be shared with the team. */
  huddleSharingBlocked: boolean;
  /** Team admins and owners may change the blocks. */
  canEdit: boolean;
};

export type CallSettings = {
  /** Kinds of call the viewer's own calls record by default. */
  recordByDefault: RecordingKinds;
  /** Huddles the viewer starts begin shared with their team. */
  shareHuddlesByDefault: boolean;
  /** The viewer refuses being recorded or transcribed in 1:1s. */
  refuseOneOnOneRecording: boolean;
  /** Absent when the viewer is not on a team. */
  team: TeamCallPolicy | null;
};

export type RecordingKindOption = {
  kind: RecordingKind;
  label: string;
  description: string;
};

export const RECORDING_KINDS: readonly RecordingKindOption[] = [
  {
    kind: 'huddles',
    label: 'Huddles',
    description: 'Calls started from a channel.',
  },
  {
    kind: 'oneOnOneMeetings',
    label: '1:1 meetings',
    description: 'Meetings with only two people, both on your team.',
  },
  {
    kind: 'internalMeetings',
    label: 'Internal meetings',
    description: 'Meetings with three or more people, all on your team.',
  },
  {
    kind: 'externalMeetings',
    label: 'External meetings',
    description: 'Meetings that guests or people outside your team join.',
  },
];

/** Whether the viewer's team forbids recording `kind`. */
export function isRecordingBlocked(
  settings: CallSettings,
  kind: RecordingKind
): boolean {
  return settings.team?.recordingBlocked[kind] ?? false;
}

/**
 * Whether the viewer's calls of `kind` start recording: they chose it and
 * their team allows it.
 */
export function recordsByDefault(
  settings: CallSettings,
  kind: RecordingKind
): boolean {
  return settings.recordByDefault[kind] && !isRecordingBlocked(settings, kind);
}

/** Whether the viewer's team forbids sharing huddles with it. */
export function isHuddleSharingBlocked(settings: CallSettings): boolean {
  return settings.team?.huddleSharingBlocked ?? false;
}

/**
 * Whether huddles the viewer starts begin shared with their team: they chose
 * it and their team allows it.
 */
export function sharesHuddlesByDefault(settings: CallSettings): boolean {
  return settings.shareHuddlesByDefault && !isHuddleSharingBlocked(settings);
}

/** `kinds` with one kind changed; the input is left untouched. */
export function withKind(
  kinds: RecordingKinds,
  kind: RecordingKind,
  value: boolean
): RecordingKinds {
  return { ...kinds, [kind]: value };
}
