-- migration: connection_profiles
-- created at: 20261030080011

CREATE TABLE connection_profiles (
    id UUID PRIMARY KEY,
    owner TEXT,
    name TEXT NOT NULL,
    engine TEXT NOT NULL CHECK (engine IN ('postgres', 'aurora_dsql', 'cockroach_db', 'yugabyte_db')),
    host TEXT NOT NULL,
    port INTEGER NOT NULL CHECK (port BETWEEN 1 AND 65535),
    database_name TEXT NOT NULL,
    username TEXT NOT NULL,
    auth_type TEXT NOT NULL CHECK (auth_type IN ('password', 'dsql_iam')),
    encrypted_secret TEXT,
    iam_region TEXT,
    iam_admin BOOLEAN,
    ssl_mode TEXT NOT NULL CHECK (ssl_mode IN ('disable', 'prefer', 'require', 'verify_ca', 'verify_full')),
    ssl_root_cert_pem TEXT,
    read_only BOOLEAN NOT NULL,
    statement_timeout_ms BIGINT CHECK (statement_timeout_ms >= 0),
    color TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (
        (auth_type = 'password' AND iam_region IS NULL AND iam_admin IS NULL)
        OR (auth_type = 'dsql_iam' AND encrypted_secret IS NULL AND iam_region IS NOT NULL AND iam_admin IS NOT NULL)
    )
);

CREATE INDEX connection_profiles_owner_updated_idx
    ON connection_profiles (owner, updated_at DESC, id);