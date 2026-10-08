-- The last chat model a user picked while other models were available.
-- A missing row means they have not chosen one. The row is deleted with the user.
CREATE TABLE user_selected_model (
    user_id text PRIMARY KEY REFERENCES "User"(id) ON DELETE CASCADE,
    model_id text NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT user_selected_model_model_id_not_empty CHECK (model_id <> '')
);
