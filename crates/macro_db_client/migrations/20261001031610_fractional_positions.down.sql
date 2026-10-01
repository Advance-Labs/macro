-- Back to zero-padded counters, in the order the keys give, and the
-- database's default collation.

WITH ranked AS (
    SELECT id, ROW_NUMBER() OVER (PARTITION BY database_id ORDER BY position, id) AS rank
    FROM database_tables
)
UPDATE database_tables target
SET position = lpad(ranked.rank::text, 12, '0')
FROM ranked
WHERE target.id = ranked.id;

WITH ranked AS (
    SELECT id, ROW_NUMBER() OVER (PARTITION BY table_id ORDER BY position, id) AS rank
    FROM database_columns
)
UPDATE database_columns target
SET position = lpad(ranked.rank::text, 12, '0')
FROM ranked
WHERE target.id = ranked.id;

WITH ranked AS (
    SELECT id, ROW_NUMBER() OVER (PARTITION BY table_id ORDER BY position, id) AS rank
    FROM database_rows
)
UPDATE database_rows target
SET position = lpad(ranked.rank::text, 12, '0')
FROM ranked
WHERE target.id = ranked.id;

ALTER TABLE database_rows ALTER COLUMN position TYPE text COLLATE "default";
ALTER TABLE database_columns ALTER COLUMN position TYPE text COLLATE "default";
ALTER TABLE database_tables ALTER COLUMN position TYPE text COLLATE "default";
