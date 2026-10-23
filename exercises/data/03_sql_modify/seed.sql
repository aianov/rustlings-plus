CREATE TABLE users (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    username   text NOT NULL UNIQUE,
    email      text NOT NULL,
    status     text NOT NULL DEFAULT 'active',
    score      integer NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO users (username, email, status, score) VALUES
    ('alice', 'alice@example.com', 'active',  100),
    ('bob',   'bob@example.com',   'active',  50),
    ('carol', 'carol@mail.org',    'banned',  0),
    ('dave',  'dave@example.com',  'active',  20),
    ('eve',   'eve@mail.org',      'pending', 0),
    ('frank', 'frank@example.com', 'pending', 0);

CREATE TABLE users_archive (
    username text NOT NULL,
    email    text NOT NULL
);

CREATE TABLE posts (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    author_id   bigint NOT NULL REFERENCES users (id),
    body        text NOT NULL,
    is_draft    boolean NOT NULL DEFAULT false,
    likes_count integer NOT NULL DEFAULT 0,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz
);

INSERT INTO posts (author_id, body, is_draft, likes_count) VALUES
    (1, 'First',     false, 3),
    (1, 'Second',    false, 0),
    (2, 'Hello',     false, 0),
    (3, 'Draft one', true,  0),
    (2, 'Draft two', true,  0),
    (4, 'Third',     false, 0);

CREATE TABLE follows (
    follower_id bigint NOT NULL REFERENCES users (id),
    followee_id bigint NOT NULL REFERENCES users (id),
    created_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (follower_id, followee_id)
);

INSERT INTO follows (follower_id, followee_id) VALUES (4, 3), (1, 3), (1, 2), (2, 1);

CREATE TABLE post_likes (
    post_id bigint NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
    user_id bigint NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    PRIMARY KEY (post_id, user_id)
);

CREATE TABLE tags (
    id   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name text NOT NULL UNIQUE
);

CREATE TABLE labels (
    id   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name text NOT NULL
);

CREATE TABLE settings (
    user_id    bigint PRIMARY KEY REFERENCES users (id),
    theme      text NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE weekly_scores (
    user_id bigint NOT NULL,
    points  integer NOT NULL
);

INSERT INTO weekly_scores (user_id, points) VALUES (1, 10), (2, 5), (5, 7), (99, 100);
