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
    created_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO posts (author_id, body, created_at) VALUES
    (1, 'rust tips', '2026-10-01 10:00:00+00'),
    (2, 'coffee',    '2026-10-03 08:00:00+00'),
    (1, 'async',     '2026-10-05 12:00:00+00'),
    (3, 'photos',    '2026-10-05 20:00:00+00'),
    (2, 'monday',    '2026-10-02 09:00:00+00'),
    (4, 'hello',     '2026-10-07 07:30:00+00'),
    (3, 'sunset',    '2026-10-01 18:00:00+00'),
    (1, 'night',     '2026-10-04 22:00:00+00'),
    (5, 'quiet',     '2026-10-06 06:00:00+00'),
    (2, 'tuesday',   '2026-10-01 07:00:00+00');

CREATE TABLE follows (
    follower_id bigint NOT NULL REFERENCES users (id),
    followee_id bigint NOT NULL REFERENCES users (id),
    PRIMARY KEY (follower_id, followee_id)
);

INSERT INTO follows VALUES
    (1, 2), (1, 3), (1, 4), (2, 1), (2, 3), (2, 4), (3, 1), (3, 4), (3, 6),
    (4, 5), (5, 4), (6, 1), (7, 1);

CREATE TABLE likes (
    post_id bigint NOT NULL REFERENCES posts (id),
    user_id bigint NOT NULL REFERENCES users (id),
    PRIMARY KEY (post_id, user_id)
);

INSERT INTO likes VALUES (1, 2), (1, 3), (2, 1), (3, 2), (3, 4), (3, 5), (4, 5), (6, 1), (6, 2);

CREATE TABLE comments (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    post_id    bigint NOT NULL REFERENCES posts (id),
    author_id  bigint NOT NULL REFERENCES users (id),
    parent_id  bigint REFERENCES comments (id),
    body       text NOT NULL,
    deleted    boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO comments (id, post_id, author_id, parent_id, body, deleted, created_at) OVERRIDING SYSTEM VALUE VALUES
    (1,  1, 2, NULL, 'nice',           false, '2026-10-01 12:00:00+00'),
    (2,  1, 3, NULL, 'agree',          false, '2026-10-01 13:00:00+00'),
    (3,  1, 2, NULL, 'nice',           false, '2026-10-01 14:00:00+00'),
    (4,  1, 4, NULL, 'late to the party', false, '2026-10-02 08:00:00+00'),
    (5,  1, 3, 1,    'thanks',         false, '2026-10-02 09:00:00+00'),
    (6,  2, 1, NULL, 'which beans?',   false, '2026-10-03 09:00:00+00'),
    (7,  6, 1, NULL, 'hi dave',        false, '2026-10-07 08:00:00+00'),
    (8,  6, 2, NULL, 'first',          false, '2026-10-07 09:00:00+00'),
    (10, 3, 4, NULL, 'who owns async?', false, '2026-10-05 13:00:00+00'),
    (11, 3, 1, 10,   'me',             false, '2026-10-05 13:10:00+00'),
    (12, 3, 5, NULL, 'great post',     false, '2026-10-05 13:20:00+00'),
    (13, 3, 2, 10,   'same here',      false, '2026-10-05 13:30:00+00'),
    (14, 3, 4, 11,   'thanks',         false, '2026-10-05 13:40:00+00'),
    (15, 3, 3, 14,   'spam',           true,  '2026-10-05 13:50:00+00'),
    (16, 3, 6, 15,   'wow',            false, '2026-10-05 14:00:00+00'),
    (17, 3, 2, 12,   'yes',            false, '2026-10-05 14:10:00+00');

SELECT setval(pg_get_serial_sequence('comments', 'id'), 100);

CREATE TABLE tags (
    id   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name text NOT NULL UNIQUE
);

INSERT INTO tags (name) VALUES ('rust'), ('sql'), ('async');

CREATE TABLE post_tags (
    post_id bigint NOT NULL REFERENCES posts (id),
    tag_id  bigint NOT NULL REFERENCES tags (id),
    PRIMARY KEY (post_id, tag_id)
);

INSERT INTO post_tags VALUES (1, 1), (3, 1), (3, 3);
