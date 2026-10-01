-- Macro Databases: views are their own typed data. A view shows one table;
-- its query and layout are the JSON of `models_databases::views::ViewQuery`
-- and `ViewLayout`. A board's cards keep their lane and their fractional key
-- there in `database_view_positions`. Both go with their table, and a card's
-- place goes with its row.

CREATE TABLE database_views (
    id UUID PRIMARY KEY,
    database_id UUID NOT NULL REFERENCES databases(id) ON DELETE CASCADE,
    table_id UUID NOT NULL REFERENCES database_tables(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    position TEXT COLLATE "C" NOT NULL,
    query JSONB NOT NULL CHECK (jsonb_typeof(query) = 'object'),
    layout JSONB NOT NULL CHECK (jsonb_typeof(layout) = 'object'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX database_views_table_name_key ON database_views (table_id, lower(name));
CREATE INDEX idx_database_views_table_position ON database_views (table_id, position);

CREATE TABLE database_view_positions (
    view_id UUID NOT NULL REFERENCES database_views(id) ON DELETE CASCADE,
    -- The option the card's lane holds, or '' for the lane of cards without one.
    lane TEXT NOT NULL,
    row_id UUID NOT NULL REFERENCES database_rows(id) ON DELETE CASCADE,
    position TEXT COLLATE "C" NOT NULL,
    PRIMARY KEY (view_id, row_id)
);

CREATE INDEX idx_database_view_positions_lane
    ON database_view_positions (view_id, lane, position);
-- A deleted row takes its places with it.
CREATE INDEX idx_database_view_positions_row ON database_view_positions (row_id);
