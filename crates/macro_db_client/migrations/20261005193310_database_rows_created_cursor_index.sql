-- Soup pages a table by (created_at, id::text). The text expression preserves
-- the shared Soup cursor ordering while allowing a bounded backward index scan.
CREATE INDEX idx_database_rows_table_created_cursor
    ON database_rows (table_id, created_at, (id::text));

-- The new index also covers lookups by table_id, including cascade deletes.
DROP INDEX idx_database_rows_table;
