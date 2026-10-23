CREATE TABLE users (
    id       bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    username text NOT NULL UNIQUE,
    karma    int NOT NULL DEFAULT 0 CHECK (karma >= 0)
);
CREATE TABLE posts (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    author_id   bigint NOT NULL REFERENCES users (id),
    body        text NOT NULL,
    status      text NOT NULL DEFAULT 'draft',
    likes_count int NOT NULL DEFAULT 0,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE tags (
    id   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name text NOT NULL UNIQUE
);
CREATE TABLE post_tags (
    post_id bigint NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
    tag_id  bigint NOT NULL REFERENCES tags (id),
    PRIMARY KEY (post_id, tag_id)
);
CREATE TABLE likes (
    post_id bigint NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
    user_id bigint NOT NULL REFERENCES users (id),
    weight  int NOT NULL DEFAULT 1,
    PRIMARY KEY (post_id, user_id)
);
CREATE TABLE audit (
    id   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    note text NOT NULL
);

INSERT INTO users (username, karma) VALUES ('anna', 50), ('boris', 5), ('clara', 0), ('dmitry', 20);

INSERT INTO posts (author_id, body, status, likes_count, created_at) VALUES
    (1, 'Первый пост про Rust',        'published', 3, now() - interval '30 hours'),
    (1, 'Rust и 100% покрытие',        'published', 1, now() - interval '5 hours'),
    (2, 'Скидка 1000 рублей',          'published', 0, now() - interval '12 hours'),
    (2, 'a_b тест',                    'draft',     0, now() - interval '20 hours'),
    (3, 'Путь C:\dir\file',            'published', 2, now() - interval '8 hours'),
    (4, 'Просто хороший день',         'published', 4, now() - interval '2 hours'),
    (4, 'Ещё один пост про Rust',      'draft',     0, now() - interval '40 hours'),
    (1, 'Ночной пост',                 'published', 2, now() - interval '15 hours');

INSERT INTO tags (name) VALUES ('rust'), ('life'), ('news');
INSERT INTO post_tags (post_id, tag_id) VALUES (1, 1), (2, 1), (7, 1), (6, 2), (8, 2), (8, 3);
INSERT INTO likes (post_id, user_id) VALUES
    (1, 2), (1, 3), (1, 4), (2, 2), (5, 1), (5, 2), (6, 1), (6, 2), (6, 3), (6, 4), (8, 2), (8, 3);
