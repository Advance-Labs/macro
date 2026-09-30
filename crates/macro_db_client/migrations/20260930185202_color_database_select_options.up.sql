-- Macro Databases: select options were created without a colour, so their pills
-- drew none. Colour each database-owned option that has none by its position
-- among its column's options, cycling through the tag palette in the order the
-- databases service now gives new options
-- (`models_properties::shared::option_color::OPTION_COLOR_CYCLE`).
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
SET color = (ARRAY[
    '#0091FF', '#46A758', '#8E4EC6', '#F76B15', '#E93D82', '#12A594',
    '#FFB224', '#3E63DD', '#E5484D', '#F5D90A', '#889096', '#E54D2E'
])[positioned.position % 12 + 1]
FROM positioned
WHERE option.id = positioned.id
  AND option.color IS NULL;
