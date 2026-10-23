CREATE TABLE users (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    username   text NOT NULL UNIQUE,
    country    text NOT NULL,
    bio        text
);

INSERT INTO users (username, country, bio) VALUES
    ('alice', 'DE', 'Пишу про Rust'),
    ('bob',   'US', NULL),
    ('carol', 'DE', 'Фото и кофе'),
    ('dave',  'FR', NULL),
    ('eve',   'US', 'Читаю чужие логи'),
    ('frank', 'DE', NULL);

CREATE TABLE posts (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    author_id  bigint NOT NULL REFERENCES users (id),
    body       text NOT NULL,
    views      integer,
    created_at timestamptz NOT NULL
);

INSERT INTO posts (author_id, body, views, created_at) VALUES
    (1, 'rust tips',  100,  '2026-09-28 10:00:00+00'),
    (1, 'second',      40,  '2026-09-28 18:00:00+00'),
    (2, 'coffee',      70,  '2026-09-29 09:00:00+00'),
    (3, 'photos',      NULL,'2026-10-01 12:00:00+00'),
    (1, 'async',      250,  '2026-10-01 20:00:00+00'),
    (2, 'monday',     120,  '2026-10-03 08:00:00+00'),
    (3, 'sunset',      30,  '2026-10-03 21:00:00+00'),
    (4, 'hello',       NULL,'2026-10-03 23:00:00+00');

CREATE TABLE likes (
    post_id    bigint NOT NULL REFERENCES posts (id),
    user_id    bigint NOT NULL REFERENCES users (id),
    created_at timestamptz NOT NULL,
    PRIMARY KEY (post_id, user_id)
);

INSERT INTO likes VALUES
    (1, 2, '2026-09-28 11:00:00+00'), (1, 3, '2026-09-29 10:00:00+00'), (1, 4, '2026-10-02 09:00:00+00'),
    (1, 5, '2026-10-03 12:00:00+00'), (2, 2, '2026-09-28 19:00:00+00'), (3, 1, '2026-09-29 10:30:00+00'),
    (3, 3, '2026-10-01 08:00:00+00'), (5, 2, '2026-10-01 21:00:00+00'), (5, 3, '2026-10-02 10:00:00+00'),
    (5, 4, '2026-10-03 15:00:00+00'), (6, 1, '2026-10-03 09:00:00+00');

CREATE TABLE comments (
    id      bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    post_id bigint NOT NULL REFERENCES posts (id),
    user_id bigint NOT NULL REFERENCES users (id),
    body    text NOT NULL
);

INSERT INTO comments (post_id, user_id, body) VALUES
    (1, 2, 'good'), (1, 3, 'nice'), (1, 4, 'thanks'), (3, 1, 'same'), (5, 2, 'yes'), (5, 5, 'agree');

CREATE TABLE follows (
    follower_id bigint NOT NULL REFERENCES users (id),
    followee_id bigint NOT NULL REFERENCES users (id),
    PRIMARY KEY (follower_id, followee_id)
);

INSERT INTO follows VALUES (2, 1), (3, 1), (4, 1), (1, 2), (5, 2), (1, 3);

CREATE TABLE tags (
    id   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name text NOT NULL UNIQUE
);

INSERT INTO tags (name) VALUES ('rust'), ('async'), ('tips'), ('coffee');

CREATE TABLE post_tags (
    post_id bigint NOT NULL REFERENCES posts (id),
    tag_id  bigint NOT NULL REFERENCES tags (id),
    PRIMARY KEY (post_id, tag_id)
);

INSERT INTO post_tags VALUES (1, 3), (1, 1), (1, 2), (3, 4), (5, 1), (5, 2);
