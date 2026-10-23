CREATE TABLE IF NOT EXISTS users (
    id BIGSERIAL PRIMARY KEY,
    username TEXT NOT NULL,
    display_name TEXT,
    bio TEXT,
    hidden BOOLEAN NOT NULL DEFAULT false,
    deleted BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS users_username_key ON users (username) WHERE NOT deleted;

CREATE TABLE IF NOT EXISTS follows (
    follower BIGINT NOT NULL REFERENCES users (id),
    followee BIGINT NOT NULL REFERENCES users (id),
    PRIMARY KEY (follower, followee),
    CONSTRAINT follows_not_self CHECK (follower <> followee)
);

CREATE INDEX IF NOT EXISTS follows_by_followee ON follows (followee);

CREATE TABLE IF NOT EXISTS mutes (
    user_id BIGINT NOT NULL REFERENCES users (id),
    muted BIGINT NOT NULL REFERENCES users (id),
    PRIMARY KEY (user_id, muted)
);

CREATE TABLE IF NOT EXISTS avatars (
    owner BIGINT PRIMARY KEY REFERENCES users (id),
    file_key TEXT NOT NULL,
    content_type TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS pending_files (
    file_key TEXT PRIMARY KEY,
    at BIGINT NOT NULL
);
