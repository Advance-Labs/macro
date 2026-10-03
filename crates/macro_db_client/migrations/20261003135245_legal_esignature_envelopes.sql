-- Account-private eSignature aggregates. Deleting the owning account deliberately
-- deletes its envelopes and PDFs; voiding/declining preserves evidence in place.
CREATE TABLE legal_envelopes (
    id UUID PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES "User"(id) ON DELETE CASCADE,
    revision BIGINT NOT NULL DEFAULT 0,
    data JSONB NOT NULL,
    source_pdf BYTEA NOT NULL,
    completed_pdf BYTEA,
    CONSTRAINT legal_envelopes_source_size CHECK (octet_length(source_pdf) <= 10485760)
);
CREATE INDEX legal_envelopes_user_id_id_idx ON legal_envelopes (user_id, id DESC);
CREATE INDEX legal_envelopes_grants_idx ON legal_envelopes USING GIN (data jsonb_path_ops);
