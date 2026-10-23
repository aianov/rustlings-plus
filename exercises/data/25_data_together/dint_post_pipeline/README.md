# Конвейер постов: Postgres, outbox и лента в Scylla

Собери весь путь поста от записи до ленты подписчиков так, чтобы сбои посередине ничего не ломали. Postgres хранит правду (посты, подписки, outbox, inbox), relay публикует события, потребитель строит ленты в Scylla. Тесты роняют брокер посередине пачки, повторяют события, доставляют их задом наперёд и запускают несколько relay сразу.

## Что уже есть

`src/lib.rs`: тип `Event` (id события из outbox, `aggregate_id` это id поста, `kind`, `payload` в JSON), типаж `Sink` (брокер), структура `Pipeline` с полями (пул Postgres, сессия Scylla, keyspace) и функция `now_us()`. Тесты в `tests/pipeline.rs` сами создают схему Postgres и keyspace Scylla и удаляют их в конце. Править тесты нельзя.

## Что сделать

1. `new`: создать в Postgres (пул уже смотрит в схему теста) таблицы, если их нет:
   - `posts (id bigserial PRIMARY KEY, author bigint NOT NULL, text text NOT NULL CHECK (length(text) > 0), deleted boolean NOT NULL DEFAULT false)`;
   - `follows (follower bigint, author bigint, PRIMARY KEY (follower, author))`;
   - `outbox (id bigserial PRIMARY KEY, aggregate_id bigint NOT NULL, kind text NOT NULL, payload jsonb NOT NULL, sent_at timestamptz)`;
   - `inbox (event_id bigint PRIMARY KEY)`;

   и в Scylla `<keyspace>.feed (user_id bigint, post_id bigint, author bigint, text text)` с ключом `(user_id, post_id)`, свежие (больший id) первыми; дождаться согласования схемы.
2. `follow`: подписка, повтор не ошибка.
3. `create_post`: пост и событие `post_created` с payload `{"author", "text", "version_us"}` одной транзакцией; вернуть id поста.
4. `delete_post`: пометить пост удалённым и записать `post_deleted` с payload `{"author", "version_us"}` одной транзакцией; удаление уже удалённого или несуществующего ничего не делает.
5. `relay_once(sink, batch)`: пачка неотправленных событий через `FOR UPDATE SKIP LOCKED`, событие поста берётся только если у этого поста нет более ранних неотправленных; публикация по порядку до первой ошибки; отметить отправленными только опубликованные; вернуть их число.
6. `consume(event)`: если событие уже в inbox, вернуть false. Иначе записать ленты: автору поста и всем его подписчикам; `post_created` это запись в feed, `post_deleted` удаление из feed, и то и другое с `USING TIMESTAMP` версии события; затем закоммитить отметку в inbox и вернуть true. Если запись в Scylla не удалась, отметка не должна закоммититься.
7. `feed(user)`: пары `(post_id, text)` ленты пользователя, свежие первыми.

## Критерии приёмки

`cargo test` при запущенных Postgres и Scylla курса:

- подписчики и автор видят посты, свежие первыми;
- несохранённый пост не порождает событий;
- падение брокера посередине пачки и падение relay после публикации не теряют событий, а повторная доставка не размножает ленту;
- удаление, доставленное раньше создания, не даёт посту воскреснуть;
- три relay одновременно публикуют каждое событие один раз.
