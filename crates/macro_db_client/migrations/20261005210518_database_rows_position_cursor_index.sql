-- The table view's stable manual order, including a unique cursor tie-breaker.
CREATE INDEX idx_database_rows_position_cursor ON database_rows (table_id, position, id);
