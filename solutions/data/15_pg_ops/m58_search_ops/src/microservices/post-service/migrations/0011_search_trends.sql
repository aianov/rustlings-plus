ALTER TABLE posts ADD COLUMN search tsvector
    GENERATED ALWAYS AS (to_tsvector('russian', body)) STORED;

CREATE INDEX posts_search ON posts USING GIN (search);

CREATE MATERIALIZED VIEW post_trends AS
    SELECT l.post, count(*) AS likes
    FROM likes l JOIN posts p ON p.id = l.post
    WHERE NOT p.deleted
      AND l.at >= (extract(epoch FROM now()) * 1000000)::bigint - 86400000000
    GROUP BY l.post;

CREATE UNIQUE INDEX post_trends_post ON post_trends (post);

CREATE FUNCTION refresh_post_trends() RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY post_trends;
END $$;
