-- Cells of a database row are entity properties (entity_type DATABASE_ROW,
-- entity_id = the row id); the row itself only carries identity and order.
-- Relations are entity-reference properties targeting rows, so the
-- junction table goes too. Existing cells and edges move over first: the
-- stored cell JSON is already a tagged PropertyValue, and an edge becomes
-- one reference in the link column's EntityReference value.
INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values)
SELECT gen_random_uuid(), r.id::text, 'DATABASE_ROW', cell.key::uuid, cell.value
FROM database_rows r, jsonb_each(r.cells) AS cell
WHERE cell.key ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
  AND EXISTS (SELECT 1 FROM property_definitions d WHERE d.id = cell.key::uuid)
ON CONFLICT (entity_id, entity_type, property_definition_id) DO NOTHING;

INSERT INTO entity_properties (id, entity_id, entity_type, property_definition_id, values)
SELECT gen_random_uuid(), l.source_row_id::text, 'DATABASE_ROW', c.property_definition_id,
       jsonb_build_object(
           'type', 'EntityReference',
           'value', jsonb_agg(
               jsonb_build_object('entity_id', l.target_row_id::text, 'entity_type', 'DATABASE_ROW')
               ORDER BY l.position NULLS LAST, l.created_at))
FROM database_row_links l
JOIN database_columns c ON c.id = l.link_column_id
GROUP BY l.source_row_id, c.property_definition_id
ON CONFLICT (entity_id, entity_type, property_definition_id) DO UPDATE SET values = EXCLUDED.values;

ALTER TABLE database_rows DROP COLUMN cells;
DROP TABLE database_row_links;
