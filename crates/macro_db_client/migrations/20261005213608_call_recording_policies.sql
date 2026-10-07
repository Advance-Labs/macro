-- Each person's call settings. A missing row records every kind of call,
-- shares huddles with their team, and allows being recorded in one-on-ones,
-- which is how calls behaved before these settings existed.
CREATE TABLE call_preferences (
    user_id TEXT PRIMARY KEY REFERENCES "User"(id) ON DELETE CASCADE,
    record_huddles BOOLEAN NOT NULL DEFAULT TRUE,
    record_one_on_one_meetings BOOLEAN NOT NULL DEFAULT TRUE,
    record_internal_meetings BOOLEAN NOT NULL DEFAULT TRUE,
    record_external_meetings BOOLEAN NOT NULL DEFAULT TRUE,
    -- Huddles they start begin with "Share with team" on. Standalone meetings
    -- are never shared with the team.
    share_huddles BOOLEAN NOT NULL DEFAULT TRUE,
    -- Refuse being recorded or transcribed in 1:1 meetings and two-person
    -- direct-message huddles, whoever hosts them.
    refuse_one_on_one_recording BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- What a team's admins forbid for everyone on the team. A missing row
-- forbids nothing.
CREATE TABLE call_team_policies (
    team_id UUID PRIMARY KEY REFERENCES team (id) ON DELETE CASCADE,
    block_recording_huddles BOOLEAN NOT NULL DEFAULT FALSE,
    block_recording_one_on_one_meetings BOOLEAN NOT NULL DEFAULT FALSE,
    block_recording_internal_meetings BOOLEAN NOT NULL DEFAULT FALSE,
    block_recording_external_meetings BOOLEAN NOT NULL DEFAULT FALSE,
    -- Huddles hosted by the team's members are never shared with the team.
    block_huddle_sharing BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Set once a third person joins a standalone call that only the host's
-- teammates have joined, so it stops counting as a one-on-one.
ALTER TABLE calls ADD COLUMN has_more_than_two_participants BOOLEAN NOT NULL DEFAULT FALSE;

-- Set once someone from outside the host's team joins a standalone call, so the
-- external-meeting recording rules govern the rest of that session.
ALTER TABLE calls ADD COLUMN has_external_participants BOOLEAN NOT NULL DEFAULT FALSE;

-- Set when a standalone call starts its recorder. A call never starts a
-- second one, so a recording stopped for privacy is not resumed.
ALTER TABLE calls ADD COLUMN recorder_claimed BOOLEAN NOT NULL DEFAULT FALSE;

-- Set when a standalone call dispatches its transcriber, so it gets one.
ALTER TABLE calls ADD COLUMN transcriber_claimed BOOLEAN NOT NULL DEFAULT FALSE;

-- People in a live one-on-one who refuse being recorded or transcribed there,
-- so everyone in the call can be told why it is not recording.
ALTER TABLE calls ADD COLUMN one_on_one_recording_refused_by TEXT[] NOT NULL DEFAULT '{}';
