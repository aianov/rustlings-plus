CREATE TABLE users (
    id       bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    username text NOT NULL UNIQUE
);

INSERT INTO users (username) VALUES ('alice'), ('bob'), ('carol'), ('dave'), ('eve'), ('frank'), ('grace');

CREATE TABLE posts (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    author_id  bigint NOT NULL REFERENCES users (id),
    body       text NOT NULL,
    likes      integer NOT NULL,
    created_at timestamptz NOT NULL
);

INSERT INTO posts (author_id, body, likes, created_at) VALUES
    (1, 'rust tips', 5, '2026-10-01 10:00:00+00'),
    (2, 'coffee',    8, '2026-10-03 08:00:00+00'),
    (1, 'async',     8, '2026-10-05 12:00:00+00'),
    (3, 'photos',    3, '2026-10-05 20:00:00+00'),
    (2, 'monday',    8, '2026-10-02 09:00:00+00'),
    (4, 'hello',     8, '2026-10-07 07:30:00+00'),
    (3, 'sunset',    0, '2026-10-01 18:00:00+00'),
    (1, 'night',     5, '2026-10-04 22:00:00+00'),
    (5, 'quiet',     1, '2026-10-06 06:00:00+00'),
    (2, 'tuesday',   3, '2026-10-01 07:00:00+00');

CREATE TABLE follows (
    follower_id bigint NOT NULL REFERENCES users (id),
    followee_id bigint NOT NULL REFERENCES users (id),
    created_at  timestamptz NOT NULL,
    PRIMARY KEY (follower_id, followee_id)
);

INSERT INTO follows VALUES
    (4, 1, '2026-10-02 11:00:00+00'),
    (2, 1, '2026-10-01 08:00:00+00'),
    (6, 1, '2026-10-04 12:00:00+00'),
    (1, 2, '2026-10-03 10:00:00+00'),
    (3, 1, '2026-10-01 15:00:00+00'),
    (5, 1, '2026-10-04 09:00:00+00'),
    (7, 1, '2026-10-04 18:30:00+00'),
    (3, 2, '2026-10-04 08:00:00+00'),
    (4, 5, '2026-10-02 12:00:00+00');

CREATE TABLE weekly_scores (
    author_id bigint NOT NULL REFERENCES users (id),
    week      integer NOT NULL,
    score     integer NOT NULL,
    PRIMARY KEY (author_id, week)
);

INSERT INTO weekly_scores VALUES
    (1, 40, 50), (2, 40, 70), (3, 40, 70), (4, 40, 40), (6, 40, 20),
    (1, 41, 80), (2, 41, 80), (3, 41, 60), (4, 41, 60), (5, 41, 30);
