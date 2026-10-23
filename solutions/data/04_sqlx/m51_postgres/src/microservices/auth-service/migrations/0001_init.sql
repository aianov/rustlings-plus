CREATE TABLE IF NOT EXISTS accounts (
    user_id BIGINT PRIMARY KEY,
    username TEXT NOT NULL,
    email TEXT,
    email_verified BOOLEAN NOT NULL DEFAULT false,
    password TEXT,
    role TEXT NOT NULL DEFAULT 'user',
    frozen BOOLEAN NOT NULL DEFAULT false,
    deleted BOOLEAN NOT NULL DEFAULT false
);

CREATE UNIQUE INDEX IF NOT EXISTS accounts_username_key ON accounts (lower(username)) WHERE NOT deleted;

CREATE TABLE IF NOT EXISTS session_families (
    family BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL,
    revoked BOOLEAN NOT NULL DEFAULT false
);

CREATE INDEX IF NOT EXISTS session_families_by_user ON session_families (user_id);

CREATE TABLE IF NOT EXISTS sessions (
    refresh_hash TEXT PRIMARY KEY,
    family BIGINT NOT NULL REFERENCES session_families (family),
    user_id BIGINT NOT NULL,
    used BOOLEAN NOT NULL DEFAULT false,
    expires BIGINT NOT NULL,
    access_jti TEXT NOT NULL,
    access_exp BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS sessions_by_jti ON sessions (access_jti);

CREATE TABLE IF NOT EXISTS denied_tokens (
    jti TEXT PRIMARY KEY,
    until BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS one_time_tokens (
    hash TEXT PRIMARY KEY,
    purpose TEXT NOT NULL,
    user_id BIGINT NOT NULL,
    expires BIGINT NOT NULL
);
