CREATE TABLE users (
    id       bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    username text NOT NULL,
    CONSTRAINT users_username_uniq UNIQUE (username)
);

CREATE TABLE posts (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    author_id   bigint NOT NULL,
    body        text NOT NULL,
    likes_count int NOT NULL DEFAULT 0,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT posts_author_fk FOREIGN KEY (author_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT posts_body_not_blank CHECK (btrim(body) <> ''),
    CONSTRAINT posts_likes_not_negative CHECK (likes_count >= 0)
);

CREATE TABLE tags (
    id   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name text NOT NULL,
    CONSTRAINT tags_name_uniq UNIQUE (name)
);

CREATE TABLE post_tags (
    post_id bigint NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
    tag_id  bigint NOT NULL REFERENCES tags (id),
    PRIMARY KEY (post_id, tag_id)
);

CREATE TABLE likes (
    post_id bigint NOT NULL,
    user_id bigint NOT NULL,
    CONSTRAINT likes_pk PRIMARY KEY (post_id, user_id),
    CONSTRAINT likes_post_fk FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE,
    CONSTRAINT likes_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX posts_created_idx ON posts (created_at DESC, id DESC);
