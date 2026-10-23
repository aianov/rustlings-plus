# Веха 55. sqlx в сервисе по-взрослому

Репозитории из m51 написаны «как получилось»: теги вставляются циклом по одному запросу, гидрация ленты из m52 добирает теги и ссылки каждого поста отдельным запросом (N+1), транзакция падает при первом же конфликте сериализации или дедлоке, а медленный запрос может держать соединение сколько угодно. Поиска постов нет, а выгрузить все свои посты можно только целиком в память.

Эта веха доводит sqlx в сервисе до рабочего вида. db-engine получает `transact`: транзакция с `SET LOCAL statement_timeout` и повтором на 40001 и 40P01 с пределом попыток, а пул берёт таймауты из конфига. Функции репозитория работают и на пуле, и в транзакции (`&mut PgConnection` и обёртка на `Acquire`), теги и ссылки пишутся одной командой через `UNNEST`, гидрация берёт их пачкой через `ANY($1)`. Поиск постов с необязательными фильтрами собирается `QueryBuilder`, сортировка только из белого списка, а `%` и `_` в тексте экранируются. Выгрузка своих постов идёт потоком: `fetch` из sqlx → gRPC server-stream → chunked-ответ шлюза.

Всё собирается из главы 11_sqlx_service (задачи sqlr, sqlr_post_repo) и задач nhr8, ntx_server_stream.

## Что сделать

1. **db-engine**: `tx/`: `TxPolicy { attempts, statement_timeout }`, `retryable` (40001, 40P01), `transact(pool, policy, |conn| Box::pin(...))` (повтор, затем `DbError::Conflict`), `DbError::Timeout` для 57014; `like/`: `escape_like`, `contains_pattern`; `DbConfig.acquire_timeout` и `statement_timeout` из `<ПРЕФИКС>_DB_ACQUIRE_MS` и `<ПРЕФИКС>_DB_STATEMENT_MS`, `DbConfig::policy`.

2. **post-service, repository**: `bulk.rs`: `write_tags` и `write_links` на `&mut PgConnection` через `unnest`, `tags_with` на любом `Acquire` (пул или транзакция), `PgPosts::assemble` собирает посты с тегами и ссылками тремя запросами на любую пачку; `by_ids`, `by_authors` и выгрузка идут через неё; `search.rs`: `Search { author, tag, text, since, sort, limit }`, `Sort { New, Old, Likes }`, трейт `SearchQueries` (`search`, `export` в `mpsc::Sender` пачками по `EXPORT_BATCH`); закрепление из m54 идёт через `transact`.

3. **post-service, service и controller**: `service/search.rs` (`SearchAsk`, сортировка только new, old, likes, текст до `MAX_SEARCH_TEXT`, предел `MAX_FOUND`, `PostError::BadSearch`), `controller/search.rs`; лайк переехал в `controller/writes.rs` (лимит 300 строк).

4. **proto**: rpc `SearchPosts(SearchRequest)` → `FeedReply`, `ExportPosts(ExportRequest)` → `stream Post`.

5. **Шлюз**: `posts.search { author?, tag?, text?, since?, sort?, limit? }`; `GET /users/me/export` под токеном отдаёт NDJSON постов chunked-ответом (`controller/export.rs`).

6. **social-cli**: `search [--tag t] [--author ник] [--sort new|old|likes] [текст]`, `export <файл>`; HTTP-клиент читает chunked-ответы.

## Приёмка

Тестам нужны Postgres и NATS курса.

- `db-engine/tests/journey/m55_sqlx_service.rs`: экранирование LIKE, повтор на 40001 и предел попыток, `statement_timeout` даёт `Timeout`.
- `post-service/tests/journey/m55_sqlx_service.rs`: `%` и `_` ищутся как буквы, фильтр по тегу и автору, сортировка по лайкам, теги приходят пачкой, одна функция на пуле и в транзакции (откат убирает), выгрузка потоком старыми первыми.
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: нет `format!` с `ORDER BY`; `push_bind`; `ESCAPE`; `unnest`; `ANY($1)`; `Acquire`; повтор на 40P01; `SET LOCAL` через `set_config`; `fetch` для выгрузки; chunked-ответ; и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

Напиши несколько постов с тегами, `search --tag rust --sort likes`, `search 100%`, `search --author alice скидка`. `export posts.ndjson` скачает все твои посты одним потоком.
