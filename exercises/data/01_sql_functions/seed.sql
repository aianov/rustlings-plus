CREATE TABLE users (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    username      text NOT NULL UNIQUE,
    email         text NOT NULL,
    display_name  text,
    status        text,
    bio           text,
    tz            text NOT NULL,
    created_at    timestamptz NOT NULL,
    last_login_at timestamptz
);

INSERT INTO users (username, email, display_name, status, bio, tz, created_at, last_login_at) VALUES
    ('alice', 'alice@example.com', '  Alice A.  ', 'active',  'Rust and coffee', 'Europe/Berlin',    '2024-01-10 10:00:00+00', '2026-09-28 10:00:00+00'),
    ('bob',   'bob@mail.org',      NULL,           'active',  NULL,              'America/New_York', '2024-03-01 03:30:00+00', '2026-09-20 08:00:00+00'),
    ('carol', 'carol@example.com', 'Carol',        'banned',  '',                'Europe/Berlin',    '2024-02-20 08:00:00+00', '2026-03-15 07:10:00+00'),
    ('dave',  'dave@mail.org',     NULL,           NULL,      '   ',             'UTC',              '2024-04-01 14:30:00+00', NULL),
    ('kenji', 'kenji@example.jp',  'Kenji',        'active',  'Tokyo based',     'Asia/Tokyo',       '2024-03-01 23:30:00+00', '2026-09-30 21:00:00+00'),
    ('eve',   'eve@example.com',   'Eve',          NULL,      NULL,              'UTC',              '2024-05-18 16:00:00+00', '2026-09-01 00:00:00+00'),
    ('frank', 'frank@mail.org',    NULL,           'pending', 'Benchmarks',      'Europe/London',    '2024-06-02 09:00:00+00', '2026-06-01 09:00:00+00'),
    ('grace', 'grace@example.com', 'grace',        'active',  'Hiker',           'Europe/Paris',     '2024-07-11 11:11:00+00', '2026-09-24 00:00:00+00'),
    ('heidi', 'heidi@example.com', NULL,           'banned',  NULL,              'UTC',              '2024-08-06 06:06:00+00', NULL);

CREATE TABLE posts (
    id         bigint PRIMARY KEY,
    author_id  bigint NOT NULL REFERENCES users (id),
    body       text NOT NULL,
    likes      integer NOT NULL,
    views      integer NOT NULL,
    created_at timestamptz NOT NULL
);

INSERT INTO posts (id, author_id, body, likes, views, created_at) VALUES
    (1,  1, 'Hello world from Berlin',  12,  100, '2026-03-01 10:00:00+00'),
    (2,  1, 'Rust is fun',              30,  120, '2026-03-17 15:45:00+00'),
    (3,  2, 'Coffee first, code later',  7,    0, '2026-03-20 08:30:00+00'),
    (4,  3, 'Reading about Postgres',   45,  300, '2026-04-02 19:20:00+00'),
    (5,  5, 'Tokyo night walk',          0,   50, '2026-04-15 12:00:00+00'),
    (6,  6, 'Benchmarks do not lie',    18,    0, '2026-05-07 15:45:00+00'),
    (7,  8, 'Sunday hike photos',      250, 1800, '2026-05-31 23:59:00+00'),
    (8,  8, 'Pasta recipe thread',      25, 2500, '2026-06-09 18:00:00+00'),
    (9,  7, 'Why I switched to Linux',  99,  990, '2026-07-10 21:10:00+00'),
    (10, 2, 'Tabs or spaces',          100, 1000, '2026-07-31 09:40:00+00'),
    (11, 1, 'Borrow checker diaries',    3,   60, '2026-08-12 20:00:00+00'),
    (12, 5, 'Morning run',               1,   10, '2026-09-19 06:30:00+00'),
    (13, 1, 'Fresh thought',             2,   20, now() - interval '2 hours'),
    (14, 2, 'Another one',               0,    5, now() - interval '5 hours'),
    (15, 3, 'Almost a day old',          1,    9, now() - interval '23 hours'),
    (16, 4, 'A day ago',                 4,    8, now() - interval '25 hours'),
    (17, 5, 'Old news',                  1,    3, now() - interval '30 hours'),
    (18, 6, 'Last week',                 2,    7, now() - interval '72 hours'),
    (19, 1, 'Exactly twenty',           20,  100, '2026-09-20 12:00:00+00');
