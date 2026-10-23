ALTER TABLE users ADD COLUMN IF NOT EXISTS followers_count BIGINT NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN IF NOT EXISTS following_count BIGINT NOT NULL DEFAULT 0;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_index
        WHERE indexrelid = to_regclass('users_username_key') AND indexprs IS NOT NULL
    ) THEN
        DROP INDEX IF EXISTS users_username_key;
        CREATE UNIQUE INDEX users_username_key ON users (lower(username)) WHERE NOT deleted;
        ALTER TABLE users ADD CONSTRAINT users_counters_check
            CHECK (followers_count >= 0 AND following_count >= 0);
        UPDATE users u SET
            followers_count = (SELECT count(*) FROM follows f WHERE f.followee = u.id),
            following_count = (SELECT count(*) FROM follows f WHERE f.follower = u.id);
    END IF;
END $$;
