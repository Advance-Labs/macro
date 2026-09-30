-- Generic per-user key-value store (crates/user_kv).
--
-- Small JSON objects owned by one user, addressed by (namespace, key), for
-- app state that doesn't warrant its own typed table: tour progress first,
-- e.g. namespace 'tours', key 'calendar'. Namespaces are free-form slugs; the
-- service enforces the value size and per-user entry limits, and these checks
-- are a backstop. Writes replace the whole value.
--
-- Rows are removed with their user. The primary key serves both "get one
-- entry" and "list a namespace" for a user.
CREATE TABLE user_kv (
    user_id TEXT NOT NULL REFERENCES "User" ("id") ON DELETE CASCADE,
    namespace TEXT NOT NULL,
    key TEXT NOT NULL,
    value JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, namespace, key),
    CONSTRAINT user_kv_namespace_slug CHECK (namespace ~ '^[a-z0-9][a-z0-9_.-]{0,63}$'),
    CONSTRAINT user_kv_key_slug CHECK (key ~ '^[a-z0-9][a-z0-9_.-]{0,63}$'),
    CONSTRAINT user_kv_value_object CHECK (jsonb_typeof(value) = 'object'),
    -- The service caps compact JSON at 16 KiB. Postgres renders jsonb as text
    -- with extra whitespace, so the backstop allows twice that.
    CONSTRAINT user_kv_value_size CHECK (octet_length(value::text) <= 32768)
);
