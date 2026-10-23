CREATE TABLE users (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    username   text NOT NULL UNIQUE,
    email      text NOT NULL,
    created_at timestamptz NOT NULL
);
CREATE TABLE posts (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    author_id  bigint NOT NULL REFERENCES users (id),
    body       text NOT NULL,
    published  boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL,
    deleted_at timestamptz
);
CREATE TABLE follows (
    follower_id bigint NOT NULL REFERENCES users (id),
    followee_id bigint NOT NULL REFERENCES users (id),
    created_at  timestamptz NOT NULL,
    PRIMARY KEY (follower_id, followee_id)
);

INSERT INTO users (username, email, created_at)
SELECT 'user' || g, 'User' || g || '@Example.com', now() - (g || ' hours')::interval
FROM generate_series(1, 5000) g;

INSERT INTO posts (author_id, body, published, created_at, deleted_at)
SELECT CASE WHEN g % 4 = 0 THEN 1 ELSE 2 + (g * 7919) % 4999 END,
       'post number ' || g,
       g % 100 <> 0,
       now() - (g || ' minutes')::interval,
       CASE WHEN g % 20 = 0 THEN now() - interval '1 day' END
FROM generate_series(1, 100000) g;

INSERT INTO follows (follower_id, followee_id, created_at)
SELECT 1 + g % 5000, 1 + (g * 31 + g / 5000) % 5000, now() - (g || ' seconds')::interval
FROM generate_series(1, 60000) g
ON CONFLICT DO NOTHING;

ANALYZE;
