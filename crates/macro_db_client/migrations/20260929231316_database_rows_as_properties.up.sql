-- Cells of a database row are entity properties (entity_type DATABASE_ROW,
-- entity_id = the row id); the row itself only carries identity and order.
-- Relations are entity-reference properties targeting rows, so the
-- junction table goes too.
ALTER TABLE database_rows DROP COLUMN cells;
DROP TABLE database_row_links;
