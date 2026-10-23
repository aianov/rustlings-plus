ALTER TABLE posts ADD COLUMN search tsvector GENERATED ALWAYS AS (
    setweight(to_tsvector('russian', title), 'A') || setweight(to_tsvector('russian', body), 'B')
) STORED;

CREATE INDEX posts_search_idx ON posts USING gin (search);

CREATE INDEX users_username_trgm_idx ON users USING gin (username gin_trgm_ops);
