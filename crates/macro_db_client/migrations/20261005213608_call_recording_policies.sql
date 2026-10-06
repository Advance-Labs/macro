-- Which kinds of call a person records by default. A missing row records every
-- kind, which is how calls behaved before these preferences existed.
CREATE TABLE call_recording_preferences (
    user_id TEXT PRIMARY KEY REFERENCES "User"(id) ON DELETE CASCADE,
    record_huddles BOOLEAN NOT NULL DEFAULT TRUE,
    record_one_on_one_meetings BOOLEAN NOT NULL DEFAULT TRUE,
    record_internal_meetings BOOLEAN NOT NULL DEFAULT TRUE,
    record_external_meetings BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Kinds of call no one on the team may record, set by team admins. A missing
-- row blocks nothing.
CREATE TABLE call_team_recording_policies (
    team_id UUID PRIMARY KEY REFERENCES team (id) ON DELETE CASCADE,
    block_huddles BOOLEAN NOT NULL DEFAULT FALSE,
    block_one_on_one_meetings BOOLEAN NOT NULL DEFAULT FALSE,
    block_internal_meetings BOOLEAN NOT NULL DEFAULT FALSE,
    block_external_meetings BOOLEAN NOT NULL DEFAULT FALSE,
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
