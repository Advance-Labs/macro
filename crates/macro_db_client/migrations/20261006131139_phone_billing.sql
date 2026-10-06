-- Phone calls in usage billing.
--
-- Answered phone minutes are recorded in ai_usage as counted audio usage under
-- the `phone_call` feature and the `pstn` model, so the existing allowance,
-- credit, overage and Stripe settlement cover them. This is the all-in
-- provider cost of a minute on a call (carrier, SIP bridge, recording and
-- transcription), in USD. Preserve any rate an administrator already set.
INSERT INTO ai_pricing (model, price_per_million_in, price_per_million_out, price_per_audio_minute)
VALUES ('pstn', 0, 0, 0.025)
ON CONFLICT (model) DO NOTHING;

-- Premium seats with the Phone add-on. Max and enterprise seats include phone
-- calling and never need a row. A row only counts while its user is a Premium
-- seat billed to `payer_id`; rows for seats that left the payer or changed plan
-- are pruned when the payer's add-on quantity is next synced with Stripe.
-- Turning the add-on off sets `ends_at` to the end of the paid period: the
-- seat keeps phone calling until then, and Stripe stops renewing it.
CREATE TABLE phone_addon_seat (
    user_id TEXT PRIMARY KEY,
    payer_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ends_at TIMESTAMPTZ
);

CREATE INDEX phone_addon_seat_payer_id_idx ON phone_addon_seat (payer_id);

-- The Phone add-on quantity last written to the payer's Stripe subscription,
-- so a change in seats can be detected without asking Stripe.
ALTER TABLE ai_billing_account
    ADD COLUMN phone_addon_quantity INTEGER NOT NULL DEFAULT 0
        CHECK (phone_addon_quantity >= 0);

-- Phone minutes included per seat, frozen alongside the AI allowance while a
-- period is open and parallel to billed_users. NULL (rows frozen before this
-- column existed) and rosters of another length include no phone minutes;
-- phone usage did not exist before this column.
ALTER TABLE ai_billing_period_allowance
    ADD COLUMN included_phone_minutes_by_user BIGINT[];
