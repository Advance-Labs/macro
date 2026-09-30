-- Clear the colours the backfill would have given, leaving any other colour.
-- Options the service coloured after the backfill match too, and lose theirs.
WITH positioned AS (
    SELECT
        option.id,
        ROW_NUMBER() OVER (
            PARTITION BY option.property_definition_id
            ORDER BY option.display_order, option.id
        ) - 1 AS position
    FROM property_options option
    JOIN property_definitions definition
        ON definition.id = option.property_definition_id
    WHERE definition.database_id IS NOT NULL
)
UPDATE property_options option
SET color = NULL
FROM positioned
WHERE option.id = positioned.id
  AND option.color = (ARRAY[
      '#0091FF', '#46A758', '#8E4EC6', '#F76B15', '#E93D82', '#12A594',
      '#FFB224', '#3E63DD', '#E5484D', '#F5D90A', '#889096', '#E54D2E'
  ])[positioned.position % 12 + 1];
