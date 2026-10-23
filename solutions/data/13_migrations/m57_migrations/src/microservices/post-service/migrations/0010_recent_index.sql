-- migrate: no-transaction
DROP INDEX CONCURRENTLY IF EXISTS posts_recent;
CREATE INDEX CONCURRENTLY posts_recent ON posts (at DESC) WHERE NOT deleted;
