# Веха 59. Чат в Scylla

Переписка живёт в журнале chat-service: каждое сообщение одной строкой, при старте весь журнал читается в память, история это срез массива. Миллион сообщений это минута старта, а «покажи переписку за прошлый год страницами» и «мои чаты, свежие первыми» в такой модели решаются только перебором. Postgres тут не лучший ответ: переписка это огромный поток записей по ключу диалога с чтением хвоста, ровно то, для чего строят Scylla.

Эта веха переносит чат в Scylla. Новый крейт `store-engine` собирает сессию из `RUSTLINGS_SCYLLA_URI`, создаёт keyspace, применяет схему из `cql/` сервиса и ждёт согласия схемы, готовит запросы при создании и переводит `QueryError` в доменные ошибки. chat-service хранит сообщения в `messages_by_chat` с партицией (диалог, месяц) и `timeuuid` по убыванию, список чатов в `chats_by_user` по последней активности (старая строка удаляется, новая вставляется), участников в `chat_members`. История листается по непрозрачному курсору из paging state и номера бакета, а вся переписка выгружается потоком через `execute_iter`.

Всё собирается из глав данных: 16..18 (задачи cql, cql_chat_messages, cqm, cqm_chat_model, scy, scy_message_store).

## Что сделать

1. **Крейт `src/crates/store-engine`** (scylla `=0.12.0`, bytes, uuid, service-runtime):
   - `config/`: `StoreConfig { uri, keyspace, connect_timeout }`, `from_env`, `read(fields, "CHAT", "chat")` (`<ПРЕФИКС>_SCYLLA_URI` или `RUSTLINGS_SCYLLA_URI`, `<ПРЕФИКС>_SCYLLA_KEYSPACE`), `good_keyspace`;
   - `session/`: `connect` (keyspace `SimpleStrategy`, согласие схемы, `use_keyspace`), `drop_keyspace`, `prepared(session, cql, идемпотентен)` с `LocalQuorum`;
   - `schema/`: `Schema = (имя, cql)`, `statements`, `apply` (ошибка названа файлом);
   - `error/`: `StoreError` (`BadCursor`, `Invalid`, `Unavailable`, `Timeout`, `Schema`, ...) из `QueryError` и `NewSessionError`;
   - `cursor/`: `Cursor { bucket, state }` как «бакет.hex», `encode_state`, `decode_state`;
   - `timeuuid/`: `timeuuid(микросекунды, соль)` версии 1 и `micros_of`.

2. **chat-service**: `cql/0001_chat.cql`; `repository/scylla.rs` и `scylla_pages.rs` (`ScyllaChats::open`, `append_at`, `chat_of`, `bucket_of` (год·12 + месяц), `ChatRepository` с прежней историей «старые первыми», трейт `ChatPages` (`page`, `chats`, `export` через `execute_iter`), outbox фактов в памяти процесса); `service/pages.rs` (`with_pages`, `page`, `chats`, `export`), `ChatError::Store` и `NoPages`; bootstrap выбирает Scylla, если задан адрес, иначе прежний журнал (до m63).

3. **proto**: `HistoryRequest.cursor = 3`, `HistoryReply.next = 2`, rpc `Chats(ChatsRequest)` → `ChatsReply { chats: [ChatSummary { peer_id, last_at, preview }] }`, `ExportChat(HistoryRequest)` → `stream Message`.

4. **Шлюз и CLI**: `chat.history` принимает `cursor` и отдаёт `next`, `chat.list` → `ChatsView`; команды `history <ник> [сколько] [курсор]` (подсказывает следующую страницу) и `chats`.

## Приёмка

Тестам нужны Postgres, Scylla (`docker start rustlings-scylla`, 127.0.0.1:59042) и NATS курса.

- `store-engine/tests/journey/m59_scylla_chat.rs`: курсор и его проверка, timeuuid по времени, разбор cql, keyspace и схема идемпотентны, сломанный файл назван.
- `chat-service/tests/journey/m59_scylla_chat.rs`: 25 сообщений в двух месячных бакетах проходятся страницами по 10 новыми первыми, прежняя история старыми первыми, список чатов по свежести без дублей, кривой курсор отбит, выгрузка через `execute_iter` отдаёт все 25.
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: `store-engine` в воркспейсе; `cql/0001_chat.cql` на месте; нет `ALLOW FILTERING`; партиция (диалог, бакет) и `CLUSTERING ORDER BY (id DESC)`; paging state в курсоре; `execute_iter`; и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

alice и bob переписываются: `history bob 10` покажет последние десять и подскажет команду для следующей страницы, `chats` выведет чаты, самые свежие первыми.
