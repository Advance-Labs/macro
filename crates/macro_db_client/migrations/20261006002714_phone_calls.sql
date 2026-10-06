-- Phone (PSTN) calls bridged into Macro calls through LiveKit SIP.
--
-- A phone call is an ordinary standalone call (no channel, no meeting link)
-- whose RTC room also holds one SIP participant: the person on the phone
-- network. Recording, transcription, summaries, naming, and sharing reuse the
-- call pipeline unchanged. Like guests, the phone leg has a live row while the
-- call is active and an archived copy written in the transaction that
-- archives the call.

-- Numbers that ring a Macro user and identify them as the caller when they
-- dial out. A number belongs to one user; freeing it when the user is deleted
-- lets it be reassigned.
CREATE TABLE phone_numbers (
    phone_number TEXT PRIMARY KEY CHECK (phone_number ~ '^\+[1-9][0-9]{6,14}$'),
    user_id TEXT NOT NULL REFERENCES "User"(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX phone_numbers_user_id ON phone_numbers(user_id);

-- The phone leg of a live call, removed with its call when it is archived.
CREATE TABLE call_phone_legs (
    call_id UUID PRIMARY KEY REFERENCES calls(id) ON DELETE CASCADE,
    direction TEXT NOT NULL CHECK (direction IN ('outbound', 'inbound')),
    -- The external party, in E.164.
    remote_number TEXT NOT NULL CHECK (remote_number ~ '^\+[1-9][0-9]{6,14}$'),
    -- The Macro number used: the caller id of an outbound call or the number
    -- an inbound caller dialed. Unknown when the trunk chose the caller id.
    local_number TEXT CHECK (local_number ~ '^\+[1-9][0-9]{6,14}$'),
    -- RTC identity of the SIP participant; transcript segments use it as
    -- their speaker id.
    participant_identity TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN (
        'dialing', 'ringing', 'active',
        'completed', 'missed', 'no_answer', 'busy', 'declined', 'failed', 'cancelled'
    )),
    -- Display snapshot of the CRM contact matched when the call started. The
    -- id is a soft reference: the CRM owns contacts, and the canonical link
    -- is the call record's Contacts property.
    crm_contact_id UUID,
    contact_name TEXT,
    -- The SIP stack's call id, for correlating carrier logs.
    sip_call_id TEXT,
    answered_at TIMESTAMPTZ,
    ended_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Archived phone legs. Archival finalizes the status, so only outcomes are
-- stored here.
CREATE TABLE call_record_phone_legs (
    call_record_id UUID PRIMARY KEY REFERENCES call_records(id) ON DELETE CASCADE,
    direction TEXT NOT NULL CHECK (direction IN ('outbound', 'inbound')),
    remote_number TEXT NOT NULL CHECK (remote_number ~ '^\+[1-9][0-9]{6,14}$'),
    local_number TEXT CHECK (local_number ~ '^\+[1-9][0-9]{6,14}$'),
    participant_identity TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN (
        'completed', 'missed', 'no_answer', 'busy', 'declined', 'failed', 'cancelled'
    )),
    crm_contact_id UUID,
    contact_name TEXT,
    sip_call_id TEXT,
    answered_at TIMESTAMPTZ,
    ended_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

-- Call history with one number, e.g. for a CRM contact.
CREATE INDEX call_record_phone_legs_remote_number ON call_record_phone_legs(remote_number);

-- Inbound calls arrive as webhooks that can repeat after a call has ended;
-- the room a SIP dispatch rule created identifies the call either way.
CREATE INDEX idx_call_records_room_name ON call_records(room_name);
