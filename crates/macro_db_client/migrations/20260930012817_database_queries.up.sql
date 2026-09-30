-- Saved database questions. A document's live query node points at a row
-- here instead of inlining its SQL. Rows are immutable: editing a question
-- saves a new row and repoints the node, so a definition never changes
-- underneath a document that embeds it.
CREATE TABLE database_queries (
    id UUID PRIMARY KEY,
    -- The database whose tables win name resolution. A query outlives its
    -- database: once the database is gone it resolves against the whole
    -- catalog, where its tables no longer exist and it reports as broken.
    database_id UUID REFERENCES databases(id) ON DELETE SET NULL,
    -- Versioned so the shape can evolve: {"version": 1, "query": "<sql>"}.
    -- A missing key's type is NULL, which a bare comparison would let pass.
    definition JSONB NOT NULL CHECK (
        jsonb_typeof(definition) = 'object'
        AND jsonb_typeof(definition -> 'version') IS NOT DISTINCT FROM 'number'
        AND jsonb_typeof(definition -> 'query') IS NOT DISTINCT FROM 'string'
    ),
    -- Provenance only; no foreign key, so a deleted user's questions keep
    -- rendering in the documents that embed them.
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Serves the ON DELETE SET NULL scan when a database is purged.
CREATE INDEX idx_database_queries_database
    ON database_queries(database_id)
    WHERE database_id IS NOT NULL;
