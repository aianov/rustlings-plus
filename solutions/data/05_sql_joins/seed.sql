CREATE TABLE users (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    username   text NOT NULL UNIQUE,
    invited_by bigint REFERENCES users (id)
);

INSERT INTO users (username, invited_by) VALUES
    ('alice', NULL),
    ('bob',   1),
    ('carol', 1),
    ('dave',  2),
    ('eve',   3),
    ('frank', 1),
    ('grace', NULL);

CREATE TABLE posts (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    author_id  bigint NOT NULL REFERENCES users (id),
    body       text NOT NULL,
    created_at timestamptz NOT NULL
);

INSERT INTO posts (author_id, body, created_at) VALUES
    (1, 'rust tips', '2026-10-01 10:00:00+00'),
    (2, 'coffee',    '2026-10-03 08:00:00+00'),
    (1, 'async',     '2026-10-05 12:00:00+00'),
    (3, 'photos',    '2026-10-05 20:00:00+00'),
    (2, 'monday',    '2026-10-02 09:00:00+00'),
    (4, 'hello',     '2026-10-07 07:30:00+00'),
    (3, 'sunset',    '2026-10-01 18:00:00+00');

CREATE TABLE follows (
    follower_id bigint NOT NULL REFERENCES users (id),
    followee_id bigint NOT NULL REFERENCES users (id),
    PRIMARY KEY (follower_id, followee_id)
);

INSERT INTO follows VALUES (1, 2), (1, 3), (1, 4), (2, 1), (3, 1), (3, 4), (4, 5), (5, 4), (6, 1);

CREATE TABLE blocks (
    blocker_id bigint NOT NULL REFERENCES users (id),
    blocked_id bigint NOT NULL REFERENCES users (id),
    PRIMARY KEY (blocker_id, blocked_id)
);

INSERT INTO blocks VALUES (1, 4), (6, 1);

CREATE TABLE likes (
    post_id bigint NOT NULL REFERENCES posts (id),
    user_id bigint NOT NULL REFERENCES users (id),
    PRIMARY KEY (post_id, user_id)
);

INSERT INTO likes VALUES (1, 2), (1, 3), (2, 1), (3, 2), (3, 4), (5, 1), (4, 5);
