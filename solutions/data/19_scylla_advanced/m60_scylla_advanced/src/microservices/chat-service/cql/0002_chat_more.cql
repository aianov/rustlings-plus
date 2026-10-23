ALTER TABLE messages_by_chat ADD edited_at bigint;

CREATE TABLE IF NOT EXISTS messages_by_id (
    id timeuuid PRIMARY KEY,
    chat text,
    bucket int,
    author bigint
);

CREATE TABLE IF NOT EXISTS chats (
    chat text PRIMARY KEY,
    first bigint,
    second bigint,
    created_at bigint,
    members set<bigint>
);

CREATE TABLE IF NOT EXISTS chat_reads (
    user_id bigint,
    peer bigint,
    last_read timeuuid,
    PRIMARY KEY (user_id, peer)
);
