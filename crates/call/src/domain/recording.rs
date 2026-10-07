//! Which calls record on their own.
//!
//! Each person chooses which kinds of call their recorder starts for by
//! default, and whether their huddles start shared with their team; team
//! admins can block recording kinds of call, or sharing huddles, for everyone
//! on the team.
//! A call records only when its host records that kind by default and the
//! host's team has not blocked it.
//!
//! Anyone can also refuse to be recorded or transcribed in one-on-ones: 1:1
//! meetings and huddles in a two-person direct message. That overrides the
//! host, but only while the call stays a one-on-one.
//!
//! A standalone call can change kind while it is live. Its recording stops
//! the first time it becomes a kind its host does not record, and starts the
//! first time it becomes one they do; a call never starts a second recording,
//! so a stopped recording is not resumed.

#[cfg(test)]
mod test;

/// The kinds of call the recording rules tell apart.
///
/// A standalone call grows through the meeting kinds in order: it is a
/// one-on-one until a third person joins, then an internal meeting, and an
/// external meeting once someone from outside the host's team joins (which can
/// happen at any point, and ends the progression).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallKind {
    /// A call started from a channel.
    Huddle,
    /// A standalone call that at most two people, all on the host's team,
    /// have joined so far.
    OneOnOneMeeting,
    /// A standalone call that three or more of the host's teammates joined.
    InternalMeeting,
    /// A standalone call that someone outside the host's team joined.
    ExternalMeeting,
}

/// One flag per [`CallKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "inbound", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct CallKinds {
    /// Calls started from a channel.
    pub huddles: bool,
    /// Standalone calls with only two people, both teammates.
    pub one_on_one_meetings: bool,
    /// Standalone calls with three or more people, all teammates.
    pub internal_meetings: bool,
    /// Standalone calls that someone outside the host's team joined.
    pub external_meetings: bool,
}

impl CallKinds {
    /// Every kind of call.
    pub const ALL: Self = Self {
        huddles: true,
        one_on_one_meetings: true,
        internal_meetings: true,
        external_meetings: true,
    };

    /// No kind of call.
    pub const NONE: Self = Self {
        huddles: false,
        one_on_one_meetings: false,
        internal_meetings: false,
        external_meetings: false,
    };

    /// Whether `kind` is one of these kinds.
    pub fn contains(self, kind: CallKind) -> bool {
        match kind {
            CallKind::Huddle => self.huddles,
            CallKind::OneOnOneMeeting => self.one_on_one_meetings,
            CallKind::InternalMeeting => self.internal_meetings,
            CallKind::ExternalMeeting => self.external_meetings,
        }
    }

    /// These kinds with `patch` applied; kinds the patch omits are unchanged.
    pub fn patched(self, patch: CallKindsPatch) -> Self {
        Self {
            huddles: patch.huddles.unwrap_or(self.huddles),
            one_on_one_meetings: patch
                .one_on_one_meetings
                .unwrap_or(self.one_on_one_meetings),
            internal_meetings: patch.internal_meetings.unwrap_or(self.internal_meetings),
            external_meetings: patch.external_meetings.unwrap_or(self.external_meetings),
        }
    }
}

/// A partial update to [`CallKinds`]. Omitted kinds keep their current value,
/// so two people changing different kinds at once do not undo each other.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(feature = "inbound", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct CallKindsPatch {
    /// New value for calls started from a channel.
    pub huddles: Option<bool>,
    /// New value for standalone calls with only two people, both teammates.
    pub one_on_one_meetings: Option<bool>,
    /// New value for standalone calls with three or more people, all teammates.
    pub internal_meetings: Option<bool>,
    /// New value for standalone calls with people from outside the team.
    pub external_meetings: Option<bool>,
}

/// The rules that decide whether a host's calls record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingRules {
    /// Kinds of call the host records by default.
    pub record_by_default: CallKinds,
    /// Kinds of call the host's team forbids recording.
    pub blocked: CallKinds,
}

impl RecordingRules {
    /// Whether a call of `kind` hosted under these rules records.
    pub fn records(&self, kind: CallKind) -> bool {
        self.record_by_default.contains(kind) && !self.blocked.contains(kind)
    }
}

impl Default for RecordingRules {
    /// Someone who never changed a setting, on a team that blocks nothing:
    /// every call records, as calls did before these settings existed.
    fn default() -> Self {
        Self {
            record_by_default: CallKinds::ALL,
            blocked: CallKinds::NONE,
        }
    }
}

/// Whether a host's huddles are shared with their team. Standalone meetings
/// are never shared with the team.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HuddleSharing {
    /// Huddles the host starts begin with "Share with team" on.
    pub share_by_default: bool,
    /// The host's team forbids sharing its members' huddles with it.
    pub blocked: bool,
}

impl HuddleSharing {
    /// Whether a huddle hosted under these rules starts shared.
    pub fn shares_by_default(&self) -> bool {
        self.share_by_default && !self.blocked
    }
}

impl Default for HuddleSharing {
    /// Someone who never changed a setting, on a team that blocks nothing:
    /// huddles start shared, as they did before these settings existed.
    fn default() -> Self {
        Self {
            share_by_default: true,
            blocked: false,
        }
    }
}

/// Everything a person's call settings decide, with their team's blocks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CallPreferences {
    /// Whether the calls they host record.
    pub recording: RecordingRules,
    /// Whether the huddles they host are shared with their team.
    pub huddle_sharing: HuddleSharing,
    /// They do not allow being recorded or transcribed in one-on-ones.
    pub refuses_one_on_one_recording: bool,
}

/// The caller's call settings.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "inbound", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct CallSettings {
    /// Kinds of call the caller's own calls record by default.
    pub record_by_default: CallKinds,
    /// Huddles the caller starts begin shared with their team.
    pub share_huddles_by_default: bool,
    /// The caller does not allow being recorded or transcribed in 1:1 meetings
    /// or two-person direct-message huddles, whoever hosts them.
    pub refuse_one_on_one_recording: bool,
    /// The caller's team policy; absent when the caller is not on a team.
    pub team: Option<TeamCallPolicy>,
}

/// What a team's admins forbid for everyone on the team.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "inbound", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct TeamCallPolicy {
    /// Kinds of call no one on the team may record.
    pub recording_blocked: CallKinds,
    /// No one's huddles may be shared with the team.
    pub huddle_sharing_blocked: bool,
    /// Whether the caller may change the blocks (team admins and owners).
    pub can_edit: bool,
}

/// Body of `PATCH /call/settings`. Omitted fields keep their current value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(feature = "inbound", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct UpdateCallSettingsRequest {
    /// Changes to the kinds of call the caller records by default.
    #[serde(default)]
    pub record_by_default: CallKindsPatch,
    /// New value for starting the caller's huddles shared with their team.
    pub share_huddles_by_default: Option<bool>,
    /// New value for refusing to be recorded or transcribed in 1:1s.
    pub refuse_one_on_one_recording: Option<bool>,
}

/// Body of `PATCH /call/settings/team`. Omitted fields keep their current
/// value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(feature = "inbound", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct UpdateTeamCallPolicyRequest {
    /// Changes to the kinds of call no one on the team may record.
    #[serde(default)]
    pub recording_blocked: CallKindsPatch,
    /// New value for forbidding sharing huddles with the team.
    pub huddle_sharing_blocked: Option<bool>,
}

/// A live standalone call that just became a later [`CallKind`]: its third
/// participant or its first from outside the host's team joined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingKindChange {
    /// The recorder attached to the call at that moment, if any.
    pub egress_id: Option<String>,
}

/// Who a live standalone call has had so far. Both flags only ever turn on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MeetingAttendance {
    /// A third person joined while everyone was on the host's team.
    pub more_than_two: bool,
    /// Someone from outside the host's team joined.
    pub external: bool,
}

impl MeetingAttendance {
    /// The kinds of call the session has been since it was `since`, oldest
    /// first, ending with its current kind.
    pub fn kinds_since(self, since: CallKind) -> Vec<CallKind> {
        let mut kinds = vec![since];
        // A call only gains its third participant while still internal, so a
        // call that is both went through the internal kind first.
        if since == CallKind::OneOnOneMeeting && self.more_than_two {
            kinds.push(CallKind::InternalMeeting);
        }
        if matches!(since, CallKind::OneOnOneMeeting | CallKind::InternalMeeting) && self.external {
            kinds.push(CallKind::ExternalMeeting);
        }
        kinds
    }
}
