CREATE TABLE IF NOT EXISTS messages_by_chat (
    chat text,
    bucket int,
    id timeuuid,
    author bigint,
    peer bigint,
    body text,
    PRIMARY KEY ((chat, bucket), id)
) WITH CLUSTERING ORDER BY (id DESC);

CREATE TABLE IF NOT EXISTS chats_by_user (
    user_id bigint,
    last_at bigint,
    peer bigint,
    preview text,
    PRIMARY KEY (user_id, last_at, peer)
) WITH CLUSTERING ORDER BY (last_at DESC, peer ASC);

CREATE TABLE IF NOT EXISTS chat_activity (
    user_id bigint,
    peer bigint,
    last_at bigint,
    PRIMARY KEY (user_id, peer)
);

CREATE TABLE IF NOT EXISTS chat_members (
    chat text,
    user_id bigint,
    PRIMARY KEY (chat, user_id)
);
