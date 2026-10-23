DROP TRIGGER posts_sync_body ON posts;
DROP FUNCTION posts_sync_body();
DROP PROCEDURE posts_backfill_body(INTEGER);
ALTER TABLE posts DROP COLUMN text;
