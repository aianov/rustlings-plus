ALTER TABLE posts ADD COLUMN body TEXT;

CREATE FUNCTION posts_sync_body() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        NEW.body := coalesce(NEW.body, NEW.text);
        NEW.text := coalesce(NEW.text, NEW.body);
    ELSIF NEW.body IS DISTINCT FROM OLD.body THEN
        NEW.text := NEW.body;
    ELSIF NEW.text IS DISTINCT FROM OLD.text THEN
        NEW.body := NEW.text;
    END IF;
    RETURN NEW;
END $$;

CREATE TRIGGER posts_sync_body BEFORE INSERT OR UPDATE ON posts
    FOR EACH ROW EXECUTE FUNCTION posts_sync_body();

CREATE PROCEDURE posts_backfill_body(batch INTEGER) LANGUAGE plpgsql AS $$
DECLARE
    moved INTEGER;
BEGIN
    LOOP
        UPDATE posts SET body = text
        WHERE id IN (SELECT id FROM posts WHERE body IS NULL LIMIT batch);
        GET DIAGNOSTICS moved = ROW_COUNT;
        EXIT WHEN moved = 0;
        COMMIT;
    END LOOP;
END $$;
