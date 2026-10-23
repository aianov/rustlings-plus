CREATE INDEX IF NOT EXISTS follows_followers_cover ON follows (followee, follower);
CREATE INDEX IF NOT EXISTS mutes_by_muted ON mutes (muted);
CREATE INDEX IF NOT EXISTS users_visible ON users (id) WHERE NOT hidden AND NOT deleted;
