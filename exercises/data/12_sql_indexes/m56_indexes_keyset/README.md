# Веха 56. Индексы и keyset-страницы

На тестовых данных всё летает, но на ста тысячах постов «посты автора» читают всю таблицу, поиск по нику без учёта регистра не может взять индекс, а лента в post-service листается через OFFSET: чем дальше страница, тем больше строк база читает и выбрасывает. Пока данных мало, этого не видно, а потом сразу видно всем.

Эта веха кладёт индексы под ключевые запросы и переводит страницы на keyset. Составной частичный индекс `posts (author, at DESC, id DESC) WHERE NOT deleted` держит посты автора, лайки и комментарии получают индексы под свои внешние ключи и выборки, подписчики читаются из покрывающего `follows (followee, follower)`, ник ищется по `lower(username)`, как построен индекс m54. Посты автора листаются по курсору `(at, id)` сколько угодно, OFFSET уходит из репозиториев. Тесты наливают тысячи строк, делают ANALYZE и проверяют `EXPLAIN (FORMAT JSON)` ключевых запросов: ни одного Seq Scan.

Всё собирается из главы 12_sql_indexes (задачи pgidx, pgidx_slow_feed).

## Что сделать

1. **Миграции**: post-service `0004_indexes.sql` (`posts_author_page`, `likes_by_user`, `comments_by_author`, `comment_likes_by_user`, `post_tags_by_tag_post`); user-service `0003_indexes.sql` (`follows_followers_cover`, `mutes_by_muted`, частичный `users_visible`).

2. **post-service**: `AUTHOR_PAGE` в `repository/search.rs`: `(at, id) < ($2, $3) ORDER BY at DESC, id DESC LIMIT $4`; `SearchQueries::author_page`; `PostService::author_page` отдаёт посты и курсор «at.id» (без базы старый путь без курсора); `AuthorPostsRequest.cursor = 3`, `FeedReply.next`; запрос ленты по авторам без OFFSET; `BY_IDS` открыт для тестов планов.

3. **user-service**: поиск по нику `lower(username) = lower($1)`; `FOLLOWERS` и `SELECT_BY_NAME` открыты для тестов планов.

4. **sqlx**: фича `json` в воркспейсе (план EXPLAIN читается как `JsonValue`).

5. **Шлюз**: `posts.by_author { target, cursor?, limit? }` → `FeedPage` с `next`.

6. **social-cli**: `posts <ник> [курсор]`, `followers <ник> [после]`; оба подсказывают команду следующей страницы.

## Приёмка

Тестам нужны Postgres и NATS курса.

- `post-service/tests/journey/m56_indexes_keyset.rs`: 5000 постов и лайков, ANALYZE; план постов автора берёт `posts_author_page`, гидрация без Seq Scan; страницы автора по 20 проходят все его посты без пропусков и повторов.
- `user-service/tests/journey/m56_indexes_keyset.rs`: 5000 пользователей и 25000 подписок; подписчики и ник без Seq Scan.
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: нет OFFSET в репозиториях; keyset `(at, id)`; составной частичный индекс; покрывающий индекс подписчиков; `lower(username)` в поиске; и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

Напиши у alice пару десятков постов: `posts alice` покажет первые десять и подскажет `posts alice <курсор>` для следующих. `followers alice` листается так же.
