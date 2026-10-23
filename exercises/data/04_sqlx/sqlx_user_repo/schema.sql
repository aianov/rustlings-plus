CREATE TABLE IF NOT EXISTS users (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    username   text NOT NULL,
    email      text NOT NULL,
    bio        text,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT users_username_uniq UNIQUE (username),
    CONSTRAINT users_email_uniq UNIQUE (email),
    CONSTRAINT users_username_len CHECK (length(username) >= 3)
);

CREATE TABLE IF NOT EXISTS posts (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    author_id  bigint NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    body       text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT posts_body_not_blank CHECK (length(trim(body)) > 0)
);

CREATE INDEX IF NOT EXISTS posts_author_idx ON posts (author_id, created_at DESC)
