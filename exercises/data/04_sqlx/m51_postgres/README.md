# Веха 51. Данные в Postgres

Соцсеть переживает перезапуск только благодаря журналам: каждый сервис пишет NDJSON и при старте проигрывает его целиком. Миллион постов это минута старта и гигабайт памяти, а найти «все посты с тегом rust за неделю» можно только перебором в памяти. Два экземпляра post-service из m50 ведут два разных журнала и видят разные посты. «Ник занят» решает словарь в памяти одного процесса, а пост и событие outbox попадают в журнал одной строкой, которую никто, кроме этого процесса, не прочтёт.

Эта веха переносит данные auth, user и post в Postgres. Новый крейт `db-engine` собирает пул из конфига, у каждого сервиса своя схема (`search_path` на каждом соединении), SQL-файлы сервиса из `migrations/` применяются по порядку при старте (пока наивно: каждый файл идемпотентен), а ошибки базы превращаются в доменные по SQLSTATE и имени ограничения. Репозитории сервисов на sqlx: SQL в константах, `query_as` и `FromRow`, `Option` для NULL, `i64` для bigint. «Ник занят» узнаётся по имени ограничения `users_username_key`, пост и его событие outbox пишутся одной транзакцией, а релей читает неотправленное из таблицы. Если база не задана, сервис работает как раньше, на журнале: так живут старые тесты и chat, feed и notify.

Всё собирается из глав данных: 00_sql_select, 01_sql_functions, 02_sql_schema, 03_sql_modify и 04_sqlx (задачи pgsel, pgfun, pgsch, pgmod, sqlx и их проекты).

## Что сделать

1. **Окружение воркспейса**: `sqlx =0.8.6` без умолчаний (`runtime-tokio`, `postgres`, `macros`); `DATABASE_URL` курса в `.env.example` и `.env`.

2. **Крейт `src/crates/db-engine`** (зависимости: sqlx, contracts, service-runtime, serde_json, tokio):
   - `config/`: `DbConfig { url (Secret), schema, max_connections }`, `DbConfig::new(url, schema)`, `pool(n)`, `DbConfig::read(fields, "USERS", "users")` (`<ПРЕФИКС>_DATABASE_URL` или `DATABASE_URL`, `<ПРЕФИКС>_DB_SCHEMA`, `<ПРЕФИКС>_DB_POOL`); без адреса базы `None`; `good_schema` (a-z, цифры, `_`, до 63 символов); `DEFAULT_DB_URL`, `PG_URL_ENV` = `RUSTLINGS_PG_URL` для тестов;
   - `pool/`: `connect(&DbConfig)`: схема создаётся, если её нет, а каждое соединение пула ставит `SET search_path TO <схема>`; `drop_schema` для тестов;
   - `migrate/`: `Migration = (имя, SQL)`, `apply(pool, &[..])` выполняет все файлы по порядку (`raw_sql`), ошибка называет файл;
   - `error/`: `DbError { Constraint { violation, constraint }, BadSchema, Unavailable, Query }`, `Violation { Unique (23505), ForeignKey (23503), Check (23514), NotNull (23502) }`, `is_unique(имя)`;
   - `outbox/`: `OUTBOX_SQL` (таблицы `outbox_versions` и `outbox`), `record_fact(соединение, факт, причина)` в транзакции вызывающего (версия агрегата растёт `ON CONFLICT DO UPDATE`), `unpublished`, `mark_published`, `waiting`.

3. **user-service**: `migrations/0001_init.sql` (`users` с частичным уникальным индексом `users_username_key ... WHERE NOT deleted`, `follows`, `mutes`, `avatars`, `pending_files`), `repository/pg.rs` (`PgUsers`: `UserRepository`, факты в той же транзакции), `pg_life.rs` (`LifecycleRepository`: скрыть, вернуть, стереть вместе с подписками), `pg_files.rs` (`AvatarRepository`, `OutboxRepository`); `MIGRATIONS`; конфиг `database` (`USERS`, схема `users`); bootstrap выбирает Postgres, если база задана.

4. **post-service**: `migrations/0001_init.sql` (`posts` с id из генератора m48, `post_tags`, `post_links`, `likes` с ключом (пост, пользователь), `comments`, `drafts`); `PgPosts::open(pool, узел)` (генератор продолжает за `max(id)`), `pg.rs` (создание с тегами и ссылками, лайк `ON CONFLICT DO NOTHING` и счётчик, правка, удаление, комментарий: всё с фактом в одной транзакции), `pg_repo.rs` (`PostRepository`, `OutboxRepository`), `pg_drafts.rs` (`DraftRepository`); конфиг `database` (`POSTS`, схема `posts`).

5. **auth-service**: `migrations/0001_init.sql` (`accounts` с `accounts_username_key` по `lower(username)`, `session_families`, `sessions`, `denied_tokens`, `one_time_tokens`); `PgAccounts` (`pg.rs`: аккаунты и одноразовые токены, `pg_sessions.rs`: открытие, ротация под `SELECT ... FOR UPDATE`, повтор гасит семейство и пишет его токены в запрет, отзыв пользователя, чистка); `bootstrap/stores.rs` выбирает хранилища; конфиг `database` (`AUTH`, схема `auth`).

6. **Слои**: в `service` и `controller` сервисов нет `sqlx::`; SQL нигде не собирается `format!("SELECT ...")` и подобным: значения только параметрами.

7. **tests/e2e**: `StackConfig.database: Option<Database { url, prefix }>`, схемы `<prefix>_users`, `<prefix>_posts`, `<prefix>_auth`, `Database::drop`.

8. **docker-compose.yml**: сервис `postgres:17-alpine` с `pg_isready`, `DATABASE_URL` у всех, user-service ждёт готовности базы.

## Дерево после вехи

```
backend/src/
  crates/db-engine/
    Cargo.toml
    src/
      lib.rs
      config/mod.rs
      error/mod.rs
      migrate/mod.rs
      outbox/mod.rs
      pool/mod.rs
  microservices/
    auth-service/migrations/0001_init.sql
    auth-service/src/bootstrap/stores.rs
    auth-service/src/repository/{pg.rs, pg_sessions.rs}
    user-service/migrations/0001_init.sql
    user-service/src/repository/{pg.rs, pg_life.rs, pg_files.rs}
    post-service/migrations/0001_init.sql
    post-service/src/repository/{pg.rs, pg_repo.rs, pg_drafts.rs}
```

Остальное дерево прежнее.

## Приёмка

Тестам нужны Postgres курса на `127.0.0.1:55432` (адрес из `RUSTLINGS_PG_URL`) и NATS курса на `127.0.0.1:54222`. Каждый тест работает в своей схеме и удаляет её за собой.

- `db-engine/tests/journey/m51_postgres.rs`: имена схем; своя схема у пула; повтор миграций; уникальность, внешний ключ и проверка становятся доменными ошибками с именем ограничения; сломанный файл назван в ошибке; плохое имя схемы.
- `user-service/tests/journey/m51_postgres.rs`: «ник занят», подписки, аудитория без скрывших, события в outbox с версиями по агрегату, данные переживают новый репозиторий, скрытие, возврат и стирание освобождают ник.
- `post-service/tests/journey/m51_postgres.rs`: теги и ссылки, лайк и повтор, комментарий, события в порядке записи, черновик, удаление, генератор после перезапуска.
- `auth-service/tests/journey/m51_postgres.rs`: ник без учёта регистра, заморозка, ротация, повтор гасит семейство, отзыв пользователя, одноразовые токены, чистка.
- `tests/e2e/tests/postgres.rs`: стек с базой, посты и лайки, полная остановка и новый стек: вход, профиль, пост и лайк на месте.
- тесты прошлых вех проходят (без базы сервисы работают на журналах), `cargo make check` зелёный.

Архитектура: `db-engine` в воркспейсе с разрешёнными зависимостями; `migrations/0001_init.sql` у трёх сервисов; нет `sqlx::` в service и controller; нет `format!` с SQL; имя ограничения для «ник занят»; факт в транзакции поста; таблица outbox; SQLSTATE; `search_path`; `FromRow`; и все договорённости прошлых вех.

## Как запустить

Postgres и NATS курса должны работать: `docker start rustlings-postgres rustlings-nats`.

```
cd backend
cargo make check
cargo make demo
```

Зарегистрируй alice и bob, напиши пост, лайкни его. `quit`, `cargo make demo` снова: пользователи, посты и лайки на месте, а `psql postgres://rustlings:rustlings@127.0.0.1:55432/rustlings -c 'SELECT id, author_name, text, likes FROM posts.posts'` покажет их прямо в базе.
