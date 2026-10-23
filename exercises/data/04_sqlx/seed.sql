CREATE TABLE users (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    username   text NOT NULL CONSTRAINT users_username_uniq UNIQUE,
    email      text NOT NULL CONSTRAINT users_email_uniq UNIQUE,
    bio        text,
    status     text NOT NULL DEFAULT 'active',
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT users_username_len CHECK (length(username) >= 3)
);

INSERT INTO users (username, email, bio, status, created_at) VALUES
    ('alice', 'alice@example.com', 'Пишу про Rust',    'active',  '2024-03-10 12:00:00+00'),
    ('bob',   'bob@example.com',   NULL,               'active',  '2023-11-02 08:30:00+00'),
    ('carol', 'carol@mail.org',    'Фото и кофе',      'banned',  '2024-01-20 19:15:00+00'),
    ('dave',  'dave@example.com',  NULL,               'pending', '2024-06-01 07:00:00+00'),
    ('eve',   'eve@mail.org',      'Читаю чужие логи', 'active',  '2022-09-15 10:00:00+00'),
    ('frank', 'frank@example.com', 'Новичок',          'pending', '2024-03-10 12:00:00+00');

CREATE TABLE posts (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    author_id  bigint NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    body       text NOT NULL CONSTRAINT posts_body_not_blank CHECK (length(trim(body)) > 0),
    likes      integer NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO posts (author_id, body, likes, created_at) VALUES
    (1, 'Первый пост про Rust', 10, '2024-03-11 09:00:00+00'),
    (1, 'sqlx это просто',       4, '2024-04-02 18:00:00+00'),
    (2, 'Утренний кофе',         7, '2024-02-14 07:30:00+00'),
    (3, 'Закат',                 0, '2024-02-20 20:10:00+00'),
    (1, 'Итоги месяца',         25, '2024-05-30 12:00:00+00'),
    (5, 'Логи не врут',         12, '2023-10-01 23:00:00+00'),
    (3, 'Кофе без сахара',       3, '2024-03-01 08:00:00+00'),
    (5, 'Новая тема',            1, '2024-06-02 10:00:00+00');

CREATE TABLE tags (
    id   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name text NOT NULL CONSTRAINT tags_name_uniq UNIQUE,
    CONSTRAINT tags_name_len CHECK (length(name) BETWEEN 1 AND 32)
);

INSERT INTO tags (name) VALUES ('rust'), ('sql'), ('coffee');

CREATE TABLE post_tags (
    post_id bigint NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
    tag_id  bigint NOT NULL REFERENCES tags (id),
    PRIMARY KEY (post_id, tag_id)
);

INSERT INTO post_tags VALUES (1, 1), (2, 2), (3, 3), (7, 3);

CREATE TABLE sessions (
    id         uuid PRIMARY KEY,
    user_id    bigint NOT NULL REFERENCES users (id),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL
);
