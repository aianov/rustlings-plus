# Веха 58. Возможности Postgres и эксплуатация

Поиск постов из m55 это `ILIKE '%слово%'`: он не знает словоформ («кошки» не найдёт «кошка»), не ранжирует и читает всю таблицу. Человека с опечаткой в нике не найти вовсе. Настройки профиля хранить негде, тренды post-service пересчитывает в памяти на каждый запрос, notify-service держит уведомления в памяти и теряет их при перезапуске, а все сервисы ходят в базу владельцем схемы и могут случайно сделать что угодно.

Эта веха берёт у Postgres то, что он умеет лучше сервиса. Посты ищутся по хранимому `tsvector` с GIN-индексом через `websearch_to_tsquery` (кавычки, минус, «or»), с рангом и подсветкой `ts_headline`. Люди находятся с опечатками через `pg_trgm`. Настройки профиля лежат в `jsonb` и обновляются частично (`settings || $2`, `Json<T>` в sqlx). Тренды считает материализованное представление, которое фоновая задача обновляет `REFRESH ... CONCURRENTLY`. notify-service переезжает в Postgres: уведомления в партициях по месяцам, партиции вперёд создаёт и старые удаляет фоновая задача. У пользователей `updated_at` ставит триггер. И эксплуатация: сервисы могут ходить ролью приложения без прав на DDL, а миграции идут владельцем (`<ПРЕФИКС>_MIGRATE_URL`), у роли свои `statement_timeout` и `idle_in_transaction_session_timeout`, обслуживание (партиции, обновление представления) идёт функциями `SECURITY DEFINER`.

Всё собирается из главы 15_pg_ops (задачи pgadv, pgadv_search, pgops, pgops_incident).

## Что сделать

1. **db-engine**: `search_path TO <схема>, public` (расширения живут в public); `connect` создаёт схему только если её нет (роль приложения не имеет права CREATE); `DbConfig.migrate_url` из `<ПРЕФИКС>_MIGRATE_URL`, `connect_owner`, `prepare(config, MIGRATIONS)`: миграции владельцем, работа ролью приложения; сервисы стартуют через `prepare`.

2. **post-service**: `0011_search_trends.sql` (`posts.search tsvector GENERATED ALWAYS AS (to_tsvector('russian', body)) STORED`, GIN, `post_trends` с уникальным индексом, `refresh_post_trends()` SECURITY DEFINER); `repository/fulltext.rs` (`FullText`: `find` с `ts_rank` и `ts_headline`, `trends` из представления, `refresh_trends`); `service/text.rs` (`find`, тренды из представления, фоновое обновление каждые `TRENDS_REFRESH`); rpc `FindPosts(FindRequest)` → `FoundReply { hits: [Hit { post, rank, headline }] }`.

3. **user-service**: `0004_search_settings.sql` (`pg_trgm` в public под общим advisory lock, GIN `gin_trgm_ops`, `settings jsonb`, `updated_at` и триггер `users_touch`); `repository/people.rs` (`ProfileExtras`: `people`, `settings`, `patch_settings`); `common/settings.rs` (`Settings` с проверкой темы и языка); `service/extras.rs`, `controller/extras.rs`, загрузка аватара и файлы переехали в `controller/files.rs`; rpc `SearchPeople`, `GetSettings`, `UpdateSettings` (proto3 `optional`). Ветка Postgres bootstrap подключает и запросы m52 (`with_queries`), и новые (`with_extras`).

4. **notify-service**: `migrations/0001_init.sql` (`notices` `PARTITION BY RANGE (at)`, `notices_ensure_partitions(back, ahead)` и `notices_drop_older(keep)` SECURITY DEFINER); `repository/pg.rs` (`PgNotices`: повтор отсекается под `pg_advisory_xact_lock` на пользователя); `NOTIFY_DATABASE_URL` или `DATABASE_URL` включает Postgres; задача обслуживания раз в час.

5. **deploy/postgres/roles.sql**: роль `social_app` с таймаутами, права на данные и функции во всех схемах, права по умолчанию для будущих таблиц.

6. **Шлюз и CLI**: `posts.find`, `users.people`, `users.settings`, `users.set_settings`; команды `text <запрос>`, `people <ник>`, `settings [поле значение]`.

## Приёмка

Тестам нужны Postgres и NATS курса.

- `post-service/tests/journey/m58_search_ops.rs`: словоформы и ранг, подсветка, минус в запросе, тренды появляются после `REFRESH CONCURRENTLY`.
- `user-service/tests/journey/m58_search_ops.rs`: «alise» находит alice, частичное обновление jsonb, триггер `updated_at`.
- `notify-service/tests/journey/m58_search_ops.rs`: запись, повтор, непрочитанные, партиции назад и вперёд, старая партиция удаляется вместе с уведомлениями.
- `db-engine/tests/journey/m58_search_ops.rs`: роль приложения пишет данные, но не DDL (42501), обслуживает через SECURITY DEFINER, `statement_timeout` висит на роли.
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: нет `CREATE` в репозиториях; хранимый tsvector, GIN, `websearch_to_tsquery`, `ts_headline`; материализованное представление и `CONCURRENTLY`; `similarity`; `settings ||`; `Json<Settings>`; триггер; партиции; `SECURITY DEFINER`; таймауты роли; и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

Напиши посты про кошек и собак: `text кошка -собака` найдёт и подсветит, `people alise` найдёт alice, `settings theme dark`, затем `settings language en` оставит тему. Роль приложения: `psql ... -f deploy/postgres/roles.sql` и `*_DATABASE_URL` с `social_app`, `*_MIGRATE_URL` с владельцем.
