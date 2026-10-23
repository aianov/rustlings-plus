# api-gateway: одна дверь для всех клиентов

Клиенты соцсети ходят в одно место: WebSocket-соединение со шлюзом, кадры `{id, service, method, data}`. Шлюз
проверяет токен при подключении, раскладывает вызовы по сервисам, собирает экран профиля из нескольких запросов,
проксирует вход и регистрацию в auth-service и не даёт одному клиенту съесть всё. Каждый кирпич ты уже писал в задачах
главы, теперь собираешь шлюз целиком. Сервисы позади шлюза скрыты за трейтом `Backend`, проверка токена за `TokenCheck`
(в проде это gRPC `ValidateToken`), апстрим auth за `Upstream`.

## Что уже есть

- Воркспейс с пакетом `src/microservices/api-gateway`.
- Готовые модули: `types.rs` (`GwError`, ответы `ok_reply` и `err_reply`, `Principal`, `TokenCheck`, `Clock`, `ManualClock`),
  `backend.rs` (трейт `Backend`: `get_user`, `list_posts`, `stats`, `create_post`, все принимают `Metadata`).
- Приёмочные тесты `tests/acceptance.rs`. Править их нельзя.

## Что сделать (везде, где `todo!()`)

1. `router.rs`: `RouterBuilder` с типизированными `route` (обработчик получает `Ctx` и данные запроса) и `route_authed`
   (обработчик получает `Principal` без `Option`; аноним получает `unauthenticated` до разбора данных). Плохие данные это
   `invalid_argument`. `build` возвращает `Err` со всеми задвоенными `service.method` (по разу, в порядке появления), какими бы
   способами они ни были заведены. `Router::call` возвращает `not_found` на неизвестный маршрут.
2. `session.rs`: `Session::open` (токен из заголовка `Authorization: Bearer` или параметра `token=`, заголовок главнее, плохой
   токен отказывает: `token_expired` или `unauthenticated`, два токена в запросе `bad_request`) и `ensure_alive` (по `now >= exp`
   соединение закрывается с `token_expired`, закрытое отвечает `closed`).
3. `metadata.rs`: `RequestIds::context` (безопасный `meta.request_id` клиента, 1..=64 символа `[A-Za-z0-9._-]`, иначе `gw-N`)
   и `forward_metadata` (`x-request-id`, затем для вошедшего `x-user-id` и `x-user-role`; из клиентских metadata проходят только
   `x-trace-id` и `accept-language` без управляющих символов и до 128 байт, ключи в нижнем регистре).
4. `profile.rs`: пользователь обязателен, посты и счётчики параллельно и необязательны: сбой или `OPTIONAL_BUDGET` (150 мс)
   превращает часть в `null`, а её имя попадает в `partial` (порядок `posts`, `stats`).
5. `proxy.rs`: `proxy` для `/auth/*`: 404 для чужого пути, 413 для тела больше `max_body`, путь без префикса `/auth`, hop-by-hop
   заголовки (и перечисленные в `Connection`) не идут ни в запрос, ни в ответ, заголовки про адрес клиента (`x-forwarded-for`,
   `x-real-ip`, `forwarded`, `x-forwarded-proto`, `x-forwarded-host`) выбрасываются и заменяются своими из адреса соединения,
   502 при ошибке апстрима, 504 по `timeout`.
6. `gateway.rs`: `Gateway::new` (регистрирует `user.get` публично, `post.create` и `profile.full` с входом; дубликаты вернули бы
   `Err`), `open`, `proxy_auth` и `serve`: кадры соединения обрабатываются параллельно, ответы уходят по готовности и несут
   `request_id`; сверх `max_in_flight` ответ `resource_exhausted` сразу; срок токена проверяется на каждом кадре, по истечении
   отвечает `token_expired` и соединение закрывается (после того как в работе завершились начатые запросы); паника в `Backend`
   даёт `internal` с id этого запроса.

## Критерии приёмки

`cargo test --workspace` проходит и `cargo clippy --workspace --all-targets -- -D warnings` чист. Тесты идут на замороженных
часах tokio, поэтому проверяют точные времена: параллельность (100 мс вместо суммы), бюджет необязательных частей (150 мс),
отказ по лимиту без ожидания, подделку metadata, срок токена и устойчивость к панике.

## Подсказки

Задачи `svgw1`..`svgw15`: почти каждый модуль здесь уже был отдельной задачей. Собирай снизу вверх: `router`, `session`,
`metadata`, `profile`, `proxy`, потом `gateway`.
