ALTER TABLE posts ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'published';
ALTER TABLE posts ADD COLUMN IF NOT EXISTS pinned BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE comments ADD COLUMN IF NOT EXISTS likes INTEGER NOT NULL DEFAULT 0;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint
        WHERE conname = 'posts_status_check' AND conrelid = 'posts'::regclass) THEN
        UPDATE posts SET status = 'deleted' WHERE deleted;
        ALTER TABLE posts ADD CONSTRAINT posts_status_check CHECK (status IN ('published', 'deleted'));
        ALTER TABLE posts ADD CONSTRAINT posts_status_matches CHECK ((status = 'deleted') = deleted);
        ALTER TABLE comments ADD CONSTRAINT comments_likes_check CHECK (likes >= 0);
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS post_authors (
    author BIGINT PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS comment_likes (
    comment BIGINT NOT NULL REFERENCES comments (id),
    user_id BIGINT NOT NULL,
    at BIGINT NOT NULL,
    PRIMARY KEY (comment, user_id)
);

CREATE INDEX IF NOT EXISTS posts_pinned_by_author ON posts (author) WHERE pinned AND NOT deleted;
