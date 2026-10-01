-- Macro Databases: tables, columns and rows were ordered by zero-padded
-- counters (`000000000001`, ...), which leave no key between two neighbours.
-- Rewrite each list's counters, in their order, into keys of the
-- `fractional_index` crate that `models_databases::position` mints them with,
-- and compare positions as bytes, which is how those keys sort.
--
-- A list of n entries takes keys of one width w, the smallest with
-- 127^w >= n: an entry's rank written in base 127, one byte 0x81 + digit per
-- digit, then the 0x80 every key ends in, all as lowercase hex.

CREATE FUNCTION pg_temp.fractional_key(rank bigint, total bigint) RETURNS text
LANGUAGE sql IMMUTABLE AS $$
    SELECT string_agg(
        to_hex(129 + (rank / power(127, width - 1 - digit)::bigint) % 127),
        '' ORDER BY digit
    ) || '80'
    FROM (
        SELECT CASE
            WHEN total <= 127 THEN 1
            WHEN total <= 16129 THEN 2
            WHEN total <= 2048383 THEN 3
            ELSE 4
        END AS width
    ) AS widths,
    generate_series(0, widths.width - 1) AS digit
$$;

ALTER TABLE database_tables ALTER COLUMN position TYPE text COLLATE "C";
ALTER TABLE database_columns ALTER COLUMN position TYPE text COLLATE "C";
ALTER TABLE database_rows ALTER COLUMN position TYPE text COLLATE "C";

WITH ranked AS (
    SELECT
        id,
        ROW_NUMBER() OVER (PARTITION BY database_id ORDER BY position, id) - 1 AS rank,
        COUNT(*) OVER (PARTITION BY database_id) AS total
    FROM database_tables
)
UPDATE database_tables target
SET position = pg_temp.fractional_key(ranked.rank, ranked.total)
FROM ranked
WHERE target.id = ranked.id;

WITH ranked AS (
    SELECT
        id,
        ROW_NUMBER() OVER (PARTITION BY table_id ORDER BY position, id) - 1 AS rank,
        COUNT(*) OVER (PARTITION BY table_id) AS total
    FROM database_columns
)
UPDATE database_columns target
SET position = pg_temp.fractional_key(ranked.rank, ranked.total)
FROM ranked
WHERE target.id = ranked.id;

WITH ranked AS (
    SELECT
        id,
        ROW_NUMBER() OVER (PARTITION BY table_id ORDER BY position, id) - 1 AS rank,
        COUNT(*) OVER (PARTITION BY table_id) AS total
    FROM database_rows
)
UPDATE database_rows target
SET position = pg_temp.fractional_key(ranked.rank, ranked.total)
FROM ranked
WHERE target.id = ranked.id;

DROP FUNCTION pg_temp.fractional_key(bigint, bigint);
