-- no-transaction
-- Supports source-scoped normalized-key pagination and bounded sibling reads.
-- Build concurrently so deployment does not block ordinary foreign-entity writers.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_foreign_entity_source_normalized_key_id
    ON foreign_entity (foreign_entity_source, (lower(foreign_entity_id) COLLATE "C"), id);
