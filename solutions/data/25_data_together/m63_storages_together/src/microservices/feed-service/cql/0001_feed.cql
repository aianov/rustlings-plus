CREATE TABLE IF NOT EXISTS feed_by_user (
    user_id bigint,
    at bigint,
    post bigint,
    PRIMARY KEY (user_id, at, post)
) WITH CLUSTERING ORDER BY (at DESC, post DESC);

CREATE TABLE IF NOT EXISTS feed_posts (
    post bigint PRIMARY KEY,
    author bigint,
    author_name text,
    body text,
    likes bigint,
    tags list<text>,
    deleted boolean,
    at bigint
);

CREATE TABLE IF NOT EXISTS followers_of (
    author bigint,
    follower bigint,
    PRIMARY KEY (author, follower)
);

CREATE TABLE IF NOT EXISTS feed_mutes (
    user_id bigint,
    muted bigint,
    PRIMARY KEY (user_id, muted)
);

CREATE TABLE IF NOT EXISTS feed_names (
    user_id bigint PRIMARY KEY,
    name text
);

CREATE TABLE IF NOT EXISTS feed_applied (
    aggregate text PRIMARY KEY,
    version bigint
);
