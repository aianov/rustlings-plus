ALTER TABLE comments ADD COLUMN IF NOT EXISTS parent BIGINT REFERENCES comments (id);
ALTER TABLE comments ADD COLUMN IF NOT EXISTS at BIGINT NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS comments_by_parent ON comments (parent) WHERE parent IS NOT NULL;
CREATE INDEX IF NOT EXISTS comments_by_post ON comments (post, id DESC) WHERE NOT removed;
CREATE INDEX IF NOT EXISTS likes_by_time ON likes (at);
