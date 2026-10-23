# Веха 63. Хранилища вместе

Соцсеть собрана из Postgres, Scylla, Redis и NATS, но склеена ещё слабо. Два экземпляра post-service из m50 читают один outbox и публикуют одни и те же события наперегонки, а релей просыпается только по таймеру. Потребитель, получивший событие дважды, может дважды создать уведомление. Ленты feed-service живут в памяти и пересобираются из всего stream при каждом старте. Счётчик лайков после write-behind из m62 может разойтись с таблицей лайков. И у каждого сервиса до сих пор есть журнал и репозитории в памяти, которые bootstrap может выбрать вместо базы.

Эта веха сводит всё в одну систему. Релей захватывает строки outbox через `FOR UPDATE SKIP LOCKED` со сроком захвата и порядком по агрегату: экземпляры не мешают друг другу, событие агрегата не обгоняет предыдущее, а строки упавшего релея по истечении срока подбирает другой. `LISTEN` и `NOTIFY` будят релей сразу после записи, опрос остаётся запасным. notify-service отмечает событие в inbox той же транзакцией, что и уведомление. Ленты feed-service лежат в Scylla строкой на подписчика, идемпотентно по времени (версии) события, а свежая страница кэшируется в Redis (`LPUSH`, `LTRIM`). Фоновая задача сверяет `likes_count` с таблицей лайков. Крейт journal уходит, bootstrap сервисов собирает только настоящие хранилища, а память остаётся тестам.

Всё собирается из главы 25_data_together (задачи dint, dint_post_pipeline, dint_consistency_audit) и задач cqm11, rdp14, svevt9–11.

## Что сделать

1. **service-runtime**: `StorageError` вместо `JournalError` (`Io`, `Broken`); крейт `src/crates/journal` удалён вместе с зависимостями.

2. **Сервисы с данными**: у каждого `repository/stores.rs` с `Stores` (набор хранилищ за трейтами, `Stores::memory()` для тестов и `Stores::postgres(..)`, `Stores::scylla(..)`); `Config.stores` вместо `Config.journal`: `Config::default()` (тесты) несёт память, `Config::from_source` (окружение) нет; bootstrap берёт базу, если она задана, иначе `config.stores`, иначе `Error::NoStorage`. Репозитории в памяти больше ничего не пишут на диск.

3. **db-engine, outbox**: `OUTBOX_CLAIMS_SQL` (захват `claimed_by`, `claimed_until`, индекс по агрегату события, триггер `outbox_wake` с `pg_notify('outbox_<схема>')`), `claim(pool, владелец, предел, срок)` через `FOR UPDATE SKIP LOCKED` и `NOT EXISTS` более раннего неопубликованного события того же агрегата, `release(pool, владелец, до)`, `wake_on_outbox(pool, signal)` на `PgListener`; репозитории user и post захватывают строки своим владельцем и слушают канал; миграции `0005_outbox_claims` у users и `0012_outbox_claims` у posts.

4. **notify-service**: `migrations/0002_inbox.sql`; `Incoming.event` (id события); `NoticeRepository::record_once`: в Postgres отметка в `inbox` и уведомление в одной транзакции, повтор это `Duplicate`.

5. **feed-service**: `cql/0001_feed.cql` (`feed_by_user`, `feed_posts`, `followers_of`, `feed_mutes`, `feed_names`, `feed_applied`); `repository/scylla.rs` и `scylla_read.rs` (`ScyllaFeed`: запись строк на подписчика `USING TIMESTAMP` версии, отсечение повторов по версии агрегата, удалённые и скрытые авторы отфильтровываются при чтении, кэш первой страницы в Redis); `FEED_SCYLLA_URI` или `RUSTLINGS_SCYLLA_URI`, `FEED_REDIS_URL` или `RUSTLINGS_REDIS_URL`.

6. **post-service**: `PgPosts::reconcile_likes` и фоновая сверка `RECONCILE_EVERY`.

## Приёмка

Тестам нужны Postgres, Scylla, Redis и NATS курса.

- `post-service/tests/journey/m63_storages_together.rs`: релей «упал», захватив всю пачку: вторая реплика ничего не берёт, пока захват держится, и публикует ровно три события после срока; `NOTIFY` будит релей без опроса; второе событие поста ждёт первое; сверка чинит испорченный счётчик.
- `feed-service/tests/journey/m63_storages_together.rs`: строка ленты на подписчика, свежие первыми, повтор события и устаревшая версия отсекаются, удалённый пост выпадает, кэш в Redis.
- `notify-service/tests/journey/m58_search_ops.rs`: inbox берёт событие один раз.
- тесты прошлых вех проходят на памяти через `Stores::memory()`; журнальные тесты переписаны на общее хранилище; `cargo make check` зелёный.

Архитектура: крейта journal нет; в bootstrap нет `Memory*`; `SKIP LOCKED`; порядок по агрегату; срок захвата; `pg_notify` и `PgListener`; inbox; строка ленты на подписчика `USING TIMESTAMP`; `LTRIM`; сверка лайков; и все договорённости прошлых вех.

## Как запустить

Нужны все хранилища курса: `docker start rustlings-postgres rustlings-scylla rustlings-redis rustlings-nats`.

```
cd backend
cargo make check
cargo make demo
```

Вся соцсеть работает на настоящих базах: перезапуск любого сервиса не теряет ни постов, ни сообщений, ни уведомлений, а два экземпляра post-service публикуют события без дублей.
