CREATE TABLE IF NOT EXISTS posts (
    id BIGINT PRIMARY KEY,
    author BIGINT NOT NULL,
    author_name TEXT NOT NULL,
    text TEXT NOT NULL,
    likes INTEGER NOT NULL DEFAULT 0 CHECK (likes >= 0),
    at BIGINT NOT NULL,
    deleted BOOLEAN NOT NULL DEFAULT false
);

CREATE INDEX IF NOT EXISTS posts_by_author ON posts (author, id DESC) WHERE NOT deleted;

CREATE TABLE IF NOT EXISTS post_tags (
    post BIGINT NOT NULL REFERENCES posts (id),
    position INTEGER NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY (post, position)
);

CREATE INDEX IF NOT EXISTS post_tags_by_tag ON post_tags (tag);

CREATE TABLE IF NOT EXISTS post_links (
    post BIGINT NOT NULL REFERENCES posts (id),
    position INTEGER NOT NULL,
    url TEXT NOT NULL,
    title TEXT,
    PRIMARY KEY (post, position)
);

CREATE TABLE IF NOT EXISTS likes (
    post BIGINT NOT NULL REFERENCES posts (id),
    user_id BIGINT NOT NULL,
    at BIGINT NOT NULL,
    PRIMARY KEY (post, user_id)
);

CREATE INDEX IF NOT EXISTS likes_by_time ON likes (at);

CREATE TABLE IF NOT EXISTS comments (
    id BIGSERIAL PRIMARY KEY,
    post BIGINT NOT NULL REFERENCES posts (id),
    author BIGINT NOT NULL,
    text TEXT NOT NULL,
    removed BOOLEAN NOT NULL DEFAULT false
);

CREATE TABLE IF NOT EXISTS drafts (
    id BIGSERIAL PRIMARY KEY,
    owner BIGINT NOT NULL,
    text TEXT NOT NULL
);
