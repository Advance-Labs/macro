-- Rows of Macro databases carry their cells as entity properties.
-- Keep this separate from any use: enum values must be committed before use.
ALTER TYPE property_entity_type ADD VALUE IF NOT EXISTS 'DATABASE_ROW';
