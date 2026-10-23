# Веха 57. Свой раннер миграций

С m51 сервисы на каждом старте прогоняют все SQL-файлы схемы подряд, и каждый файл обязан быть идемпотентным. В m54 это уже скрипело: разовые переносы пришлось прятать в `DO $$ IF NOT EXISTS ... $$`. Дальше так нельзя: два экземпляра post-service стартуют одновременно и мигрируют наперегонки, никто не помнит, какие файлы применены, правка старого файла молча расходится с базой, а `CREATE INDEX CONCURRENTLY` вообще не работает внутри транзакции.

Эта веха заменяет наивное применение своим раннером в db-engine. Файлы `NNNN_имя.sql` применяются по порядку версий, каждая применённая версия записана в `schema_migrations` с контрольной суммой, каждая миграция идёт своей транзакцией, а файл с пометкой `-- migrate: no-transaction` выполняется по командам вне транзакции (для CONCURRENTLY). Мигрирует один экземпляр: advisory lock на схему. Миграция ждёт чужие замки не дольше `lock_timeout` и повторяется. И на этом раннере `posts.text` переезжает в `posts.body` без простоя: expand (новая колонка и триггер, который держит обе в согласии, пока работает старый код), backfill пачками с COMMIT между ними, обязательность через `NOT VALID` и отдельный `VALIDATE`, затем contract.

Всё собирается из главы 13_migrations (задачи pgmig, pgmig_runner, pgmig_zero_downtime).

## Что сделать

1. **db-engine, migrate/**: `migrate(pool, &[Migration]) -> Vec<имена применённых>` (таблица `schema_migrations (version, name, checksum, applied_at)`, `pg_advisory_lock(hashtext(current_schema()))` на одном соединении, версии строго по возрастанию, `version(имя)` из четырёх цифр, изменённый файл это `DbError::Migration`, ошибка SQL названа именем файла), транзакция на миграцию с `set_config('lock_timeout', ..., true)`, файлы `-- migrate: no-transaction` по командам, повтор на 55P03 и 40P01 до `LOCK_ATTEMPTS` с паузой; `checksum.rs` (FNV-1a своими руками); `applied(pool)`; `apply` теперь тонкая обёртка над раннером.

2. **post-service, миграции**: `0005_body_expand.sql` (колонка `body`, триггер `posts_sync_body` в обе стороны, процедура `posts_backfill_body(batch)` с COMMIT между пачками), `0006_body_backfill.sql` (без транзакции: `CALL`), `0007_body_check.sql` (`CHECK (body IS NOT NULL) NOT VALID`), `0008_body_validate.sql` (`VALIDATE CONSTRAINT`, затем `SET NOT NULL` без полного прохода), `0009_body_contract.sql` (триггер, процедура и `text` уходят), `0010_recent_index.sql` (без транзакции: `CREATE INDEX CONCURRENTLY`); SQL репозиториев читает и пишет `body`.

3. **Сервисы**: bootstrap auth, user и post вызывает `db_engine::migrate`.

4. **xtask**: `cargo xtask migrate` применяет миграции auth, users и posts по `<ПРЕФИКС>_DATABASE_URL` или `DATABASE_URL`; задача `[tasks.migrate]`; сервисы с данными в `[workspace.dependencies]`.

## Приёмка

Тестам нужны Postgres и NATS курса.

- `db-engine/tests/journey/m57_migrations.rs`: два раннера одновременно применяют набор один раз; контрольные суммы записаны; изменённый файл, неверный порядок и имя без версии отбиты; упавшая миграция откатилась; CONCURRENTLY без транзакции; миграция, упёршаяся в чужой замок, проходит после повтора.
- `post-service/tests/journey/m57_migrations.rs`: старый код пишет `text` до и после expand, backfill переносит 2500 строк пачками, триггер держит обе колонки, после contract `text` нет, `body` обязателен, репозиторий работает.
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: refinery и sqlx-cli запрещены; нет `sqlx::migrate`; в db-engine нет комментариев; `schema_migrations`; advisory lock; `lock_timeout`; пометка без транзакции; `NOT VALID` и `VALIDATE`; COMMIT в backfill; и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make migrate
cargo make demo
```

`cargo make migrate` второй раз скажет, что схемы уже на месте. В демо `devctl restart` поднимает сервисы заново: оба экземпляра post-service стартуют, мигрирует один, клиент ошибок не видит.
