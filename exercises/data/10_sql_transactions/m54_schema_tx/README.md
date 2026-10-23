# Веха 54. Схема по правилам и счётчики без гонок

Схема из m51 работает, пока запросы идут по одному. Два встречных «подписаться» могут сцепиться замками, счётчики подписчиков считаются заново на каждый показ карточки, ник «Alice» проходит рядом с «alice», статус поста ничем не проверен, а лимит «не больше трёх закреплённых постов» при двух одновременных запросах превращается в четыре: обе транзакции видят по два закреплённых и обе добавляют свой (write skew).

Эта веха приводит схему в порядок и убирает гонки. Сервисы не ссылаются на чужие схемы внешними ключами, ник уникален по `lower()`, мягкое удаление остаётся с частичным уникальным индексом, у поста статус с CHECK, у комментариев лайки. Счётчики подписок лежат рядом с данными и меняются в той же транзакции, что и подписка, а замки двух строк пользователей берутся в одном порядке (по id), поэтому встречные подписки не дают дедлока. Лайк и подписка идемпотентны (`ON CONFLICT`), правка поста держит `FOR NO KEY UPDATE`, а закрепление постов идёт под `FOR UPDATE` строки автора. Новые файлы схемы ещё ложатся наивным применением из m51, но уже со скрипом: каждый старт заново проверяет, нужен ли разовый перенос, и параллельные схемы в тестах требуют аккуратных проверок каталога.

Всё собирается из глав данных: 09_sql_modeling и 10_sql_transactions (задачи pgmdl, pgmdl_services_schema, pgtx, pgtx_likes_race).

## Что сделать

1. **user-service**: `migrations/0002_counters.sql`: столбцы `followers_count` и `following_count`, разовый перенос в `DO $$ ... $$` (уникальный `users_username_key` по `lower(username) WHERE NOT deleted`, CHECK на неотрицательные счётчики, пересчёт счётчиков), проверка через `to_regclass` своей схемы; поиск по нику через `lower()`; `repository/pg_follow.rs`: замки `SELECT id FROM users WHERE id = ANY($1) ORDER BY id FOR UPDATE`, вставка `ON CONFLICT DO NOTHING`, счётчики одной командой в той же транзакции; `Counters` читает столбцы.

2. **post-service**: `migrations/0003_pins.sql`: `posts.status` с `CHECK (status IN ('published', 'deleted'))` и согласием с `deleted`, `posts.pinned`, `comments.likes`, таблицы `post_authors` (строка для замка) и `comment_likes`; `MIGRATIONS` переехал в `repository/schema.rs`; правка через `FOR NO KEY UPDATE`, удаление ставит статус и снимает закрепление; `repository/pins.rs` с трейтом `PinQueries` (`pin`, `pinned`, `like_comment`), `MAX_PINS = 3`, исходы `Pinned { Changed, Missing, NotAuthor, Full }`; `service/pins.rs` (`PostError::TooManyPins` это Conflict); `controller/pins.rs`, а создание, правка и удаление переехали в `controller/writes.rs` (лимит 300 строк).

3. **proto**: rpc `Pin`, `Unpin` (`PostAction` → `Changed`), `PinnedPosts(AuthorPostsRequest)` → `FeedReply`, `LikeComment(CommentAction)` → `CommentLikeReply { changed, likes }`.

4. **Шлюз**: `posts.pin`, `posts.unpin`, `posts.like_comment` для вошедших, `posts.pinned { target }`; `CommentLikeView` в contracts.

5. **social-cli**: `pin <пост>`, `unpin <пост>`, `pinned <ник>`, `like-comment <комментарий>`.

## Приёмка

Тестам нужны Postgres и NATS курса.

- `post-service/tests/journey/m54_schema_tx.rs`: шесть одновременных закреплений дают ровно три, чужой пост не закрепить, после открепления место есть, лайк комментария идемпотентен, неверный статус отбит CHECK (23514).
- `user-service/tests/journey/m54_schema_tx.rs`: все встречные подписки шести пользователей параллельно без дедлока, счётчики сходятся, повтор не двигает счётчик, «USER0» рядом с «user0» отбит (23505).
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: нет внешних ключей на чужие схемы; `lower(username)`; порядок замков; счётчики в транзакции; `FOR NO KEY UPDATE`; замок строки автора; `ON CONFLICT`; CHECK статуса; и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

alice пишет четыре поста и закрепляет их по одному: четвёртый `pin` получает отказ, `unpin` освобождает место, `pinned alice` показывает закреплённые. bob и alice подписываются друг на друга, `card alice` показывает счётчики из столбцов, `like-comment` лайкает комментарий.
