CREATE INDEX IF NOT EXISTS posts_author_page ON posts (author, at DESC, id DESC) WHERE NOT deleted;
CREATE INDEX IF NOT EXISTS likes_by_user ON likes (user_id, post);
CREATE INDEX IF NOT EXISTS comments_by_author ON comments (author);
CREATE INDEX IF NOT EXISTS comment_likes_by_user ON comment_likes (user_id);
CREATE INDEX IF NOT EXISTS post_tags_by_tag_post ON post_tags (tag, post);
