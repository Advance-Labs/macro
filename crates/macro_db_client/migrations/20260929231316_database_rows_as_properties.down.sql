CREATE TABLE database_row_links (
    link_column_id UUID NOT NULL REFERENCES database_columns(id) ON DELETE CASCADE,
    source_row_id UUID NOT NULL REFERENCES database_rows(id) ON DELETE CASCADE,
    target_row_id UUID NOT NULL REFERENCES database_rows(id) ON DELETE CASCADE,
    position TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (link_column_id, source_row_id, target_row_id)
);
CREATE INDEX idx_database_row_links_source ON database_row_links(source_row_id);
CREATE INDEX idx_database_row_links_target ON database_row_links(target_row_id);
ALTER TABLE database_rows ADD COLUMN cells JSONB NOT NULL DEFAULT '{}'::jsonb;
