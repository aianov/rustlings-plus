# Веха 60. Чат глубже

Чат из m59 хранит и листает переписку, но живые чаты сложнее. Два первых сообщения навстречу друг другу заводят чат дважды. Правка сообщения, дошедшая позже более новой правки, молча откатывает текст назад. Исчезающих сообщений нет, непрочитанное не считается, а сообщение пишется в две таблицы двумя запросами: упади сервис между ними, и одна таблица разойдётся с другой.

Эта веха использует то, что Scylla даёт для таких случаев. Личный чат двоих создаётся лёгкой транзакцией `IF NOT EXISTS` один раз, участники копятся в `set` добавлением без перезаписи. Сообщение пишется в `messages_by_chat` и `messages_by_id` одним logged batch, с `USING TTL` для исчезающих и `USING TIMESTAMP` времени сообщения. Правка идёт `UPDATE ... USING TIMESTAMP` времени правки, поэтому опоздавшая старая правка не затирает новую. Непрочитанное считается от `last_read` с пределом «99+», а уровни консистентности выбраны по операциям: записи `LocalQuorum`, списки и счётчики `LocalOne`.

Всё собирается из главы 19_scylla_advanced (задачи scya, scya_presence_unread).

## Что сделать

1. **store-engine**: `apply` пропускает повтор `ALTER TABLE ... ADD` (колонка уже есть), `already_there`.

2. **chat-service, схема** `cql/0002_chat_more.cql`: `messages_by_chat.edited_at`, `messages_by_id` (id → диалог, бакет, автор), `chats (chat, first, second, created_at, members set<bigint>)`, `chat_reads (user_id, peer, last_read timeuuid)`.

3. **chat-service, репозиторий**: `Message.id` (timeuuid строкой) и `edited`; `append_for(сообщение, время, ttl)`: чат открывается один раз (`open_chat` с `IF NOT EXISTS` и `members = members + ?`), logged batch в две таблицы `USING TTL ? AND TIMESTAMP ?`; `repository/scylla_more.rs`: `edit_at` (`USING TIMESTAMP`, только автор), `unread_of` (`id > ? LIMIT ?`, свои не считаются, `UNREAD_CAP = 100`), трейт `ChatExtras` (`send_for`, `edit`, `mark_read`, `unread`); список чатов несёт `unread`.

4. **chat-service, сервис и proto**: `service/extras.rs` (`send_for` с `MAX_TTL`, `edit`, `mark_read`, ошибки `UnknownMessage`, `NotAuthor`, `BadTtl`); `SendRequest.ttl_seconds`, `Message.id`, `Message.edited`, `ChatSummary.unread`, rpc `EditMessage`, `MarkRead`.

5. **Шлюз и CLI**: `chat.send` с `ttl`, `chat.edit`, `chat.seen`, `MessageView.id` и `edited`, `ChatLineView.unread`; команды `msg <ник> --ttl 30s <текст>`, `edit <ник> <id> <текст>`, `seen <ник>`, у `chats` счётчик непрочитанного с «99+», у сообщений в истории id и пометка «изменено».

## Приёмка

Тестам нужны Postgres, Scylla и NATS курса.

- `chat-service/tests/journey/m60_scylla_advanced.rs`: повторный старт со схемой проходит, чат открывается один раз, новая правка побеждает опоздавшую старую, чужую правку не принять, участники в set, 105 непрочитанных показываются как предел, свои не считаются, прочтение обнуляет, сообщение с TTL 1 секунда исчезает.
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: нет `UNLOGGED BATCH`; `BatchType::Logged`; `IF NOT EXISTS`; `members = members + ?`; `USING TTL`; `USING TIMESTAMP` у правки; счёт от `last_read` с пределом; консистентность по операциям; и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

`history bob` покажет id сообщений; `edit bob <id> поправленный текст` поправит своё. `msg bob --ttl 30s секрет` исчезнет через полминуты. У bob в `chats` виден счётчик непрочитанного, `seen alice` его обнулит.
