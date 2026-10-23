# Веха 30. Монолит делится

Шлюз умеет всё сразу: выдаёт токены, хранит профили, посты, аватары и переписку, держит сокеты. Пока это один процесс, любая правка во входе пересобирает и перезапускает ленту, а упавший журнал постов кладёт и регистрацию. Пора резать: вход, профили и посты становятся отдельными сервисами со своими журналами, а шлюз остаётся тем, чем ему и положено быть, переводчиком между WebSocket клиента и gRPC сервисов.

Здесь собирается всё из главы 14: общий крейт со схемами, сервер и клиент tonic, ленивый `Channel` на сервис, `Status` с кодом и без утечки чужого текста, `x-request-id` в метаданных, поднятый лимит размера сообщения и два сервиса, которые зовут друг друга.

Для сборки нужен `protoc` (`protoc --version`; на macOS `brew install protobuf`, в Debian и Ubuntu `apt install protobuf-compiler`).

## Что сделать

1. **Корень.**
   - В `[workspace.dependencies]`: `prost = "=0.14.4"`, `tonic = "=0.14.6"`, `tonic-prost = "=0.14.6"`, `tonic-prost-build = "=0.14.6"`, `tokio-stream = { version = "0.1", features = ["net"] }` и пути `proto`, `auth-service`, `user-service`, `post-service`.
   - Заготовки сервисов: `cargo xtask new-service user-service`, `post-service`, `auth-service`.
   - `.env.example` (и твой `.env`, тест m09 сверяет ключи): `AUTH_HTTP_ADDR=127.0.0.1:7881`, `AUTH_GRPC_ADDR=127.0.0.1:50051`, `USERS_GRPC_ADDR=127.0.0.1:50052`, `POSTS_GRPC_ADDR=127.0.0.1:50053`, в комментариях `USERS_JOURNAL`, `POSTS_JOURNAL`, `AUTH_JOURNAL`; `SOCIAL_JOURNAL` уходит.
   - `Makefile.toml`: задачи `build-all` (`cargo build --workspace --bins`), по задаче на сервис (`user-service`, `post-service`, `auth-service`, `api-gateway`), `run-all` с `dependencies = ["build-all"]` и `run_task = { name = [...], parallel = true }`. `demo` собирает всё, поднимает четыре процесса в фоне, запускает клиента и по выходу гасит процессы через `kill -INT` (тогда журналы успевают дописаться). В `Makefile` такие же `run-all` и `demo`.

2. **src/crates/proto** (новый крейт, зависимости только `prost`, `tonic`, `tonic-prost`, `getrandom`, сборка `tonic-prost-build`).
   - `proto/social/v1/{common,auth,users,posts}.proto` приходят готовыми: это контракт, его не правят.
   - `build.rs` компилирует четыре файла с корнем `proto`, `lib.rs`: `pub mod social { pub mod v1 { tonic::include_proto!("social.v1"); } }`.
   - `endpoints/`: `Endpoint { env, default }` с `addr()` (переменная окружения или адрес по умолчанию), `AUTH` (`AUTH_GRPC_ADDR`, `127.0.0.1:50051`), `USERS` (`USERS_GRPC_ADDR`, `:50052`), `POSTS` (`POSTS_GRPC_ADDR`, `:50053`), `AVATAR_MAX = 8 МиБ`, `MESSAGE_MAX = AVATAR_MAX + 64 КиБ`.
   - `status/`: `ERROR_CODE = "x-error-code"`, `REQUEST_ID = "x-request-id"`, и функции:

   | функция | что делает |
   |---|---|
   | `domain(code, message) -> Status` | `not_found` → NOT_FOUND, `conflict` → ALREADY_EXISTS, `forbidden` → PERMISSION_DENIED, `unauthorized` → UNAUTHENTICATED, `unavailable` → UNAVAILABLE, `rejected` → FAILED_PRECONDITION, `internal` → `internal()`, остальное → INVALID_ARGUMENT; код кладётся в метаданные `x-error-code` |
   | `internal() -> Status` | INTERNAL с общим текстом «внутренняя ошибка сервиса» |
   | `domain_code(&Status) -> Option<&str>` | код из `x-error-code`, кроме `internal` |
   | `explain(&Status, service) -> (String, String)` | код и текст для клиента: доменный код с текстом статуса; UNAVAILABLE, DEADLINE_EXCEEDED и CANCELLED → `unavailable` и «{service} недоступен, повтори позже»; всё прочее → `internal` и общий текст. Чужой текст без `x-error-code` никогда не пересказывается |
   | `request_id(&MetadataMap) -> String` | `x-request-id` или `-` |
   | `new_request_id() -> String` | 8 случайных байт `getrandom` в hex, 16 знаков |
   | `traced(message, request_id) -> Request<T>` | запрос с `x-request-id` в метаданных |

3. **social-core** остаётся общей библиотекой и учится отвечать про связи.
   - `Relations { following, followers, muted, audience }` в портах: подписки по порядку, подписчики по порядку, скрытые по возрастанию id, `audience` это подписчики, которые не скрыли пользователя (им уходит пуш о посте).
   - `UserRepo::relations(id) -> BoxFuture<Relations>` с телом по умолчанию (пустые связи), `MemRepo` отвечает через `Network::relations`, `App::relations(id)`. `SlowUsers` из test-support пробрасывает его, как все методы.
   - `Chats` экспортируется из крейта: переписка пока живёт в шлюзе.

4. **http-engine.** `bearer_auth_async(verify, guard)`: `verify: Fn(String) -> BoxFuture<Result<Option<String>, Response>>`. Чужой `X-User` стирается всегда, но проверка идёт только на путях, где `guard` true: каждая проверка теперь сетевой вызов. Нет токена или `Ok(None)` это 401 с `WWW-Authenticate: Bearer`, `Err(response)` заканчивает запрос этим ответом (например 503), `Ok(Some(user))` кладёт пользователя в `X-User`.

5. **user-service** (`src/microservices/user-service`): профили, подписки, скрытые авторы и аватары.
   - `run(listener, Config, CancellationToken) -> Result<()>`: `App` ядра (`App::restore` по журналу или `App::start`), сервер tonic с `serve_with_incoming_shutdown`, `UsersServer` с `max_decoding_message_size(MESSAGE_MAX)` и `max_encoding_message_size(MESSAGE_MAX)`: аватар идёт одним сообщением, и лимит поднят осознанно. После остановки сервера `app.close(drain)`.
   - `Config { journal, drain }` с `Default` и `from_env` (`JOURNAL_ENV = "USERS_JOURNAL"`), свой `Error` (`Core`, `Bind`, `Serve`, `Io`) с `source` и `Result`.
   - `UsersService::new(app)`: каждый метод это вызов `App`, ошибка ядра уходит как `domain(error.code(), &report(&error))`. `ListProfiles(after, limit)`: профили с id больше `after` подряд, `limit` 0 значит `LIST_LIMIT = 500`, больше `LIST_LIMIT` не отдаётся, `after = u64::MAX` даёт пусто без паники. `Relations` несуществующего это NOT_FOUND. Аватары в `avatars/` (`Avatars`, `Avatar` из шлюза): не `image/*` это `unsupported`, больше `AVATAR_MAX` это `too_large`, чужой id это NOT_FOUND.
   - `main.rs`: `TcpListener::bind(USERS.addr())`, строка о старте, остановка по Ctrl-C.

6. **post-service**: посты, лайки, комментарии, тренды и лента.
   - `run(listener, Config { users, journal, drain }, token)`: канал к user-service создаётся один раз через `connect_lazy`, `JOURNAL_ENV = "POSTS_JOURNAL"`, свой `Error` с `Upstream` на кривой адрес.
   - `mirror/`: `Mirror` держит клиента user-service. Своему `App` post-service нужны профили и подписки, поэтому он держит их зеркало. `profile(id)` спрашивает `GetProfile`. `ensure(app, id)` подтягивает недостающих по порядку: `ListProfiles` после `user_count`, регистрация ников подряд, выданный id сверяется с id профиля (не совпал: `internal` и строка в stderr), пустая страница значит такого пользователя нет. Зеркалирование идёт под `tokio::sync::Mutex`. `sync_viewer(app, viewer)` перед лентой берёт `Relations` и повторяет подписки и скрытия читателя.
   - `PostsService`: `CreatePost` всегда спрашивает user-service, есть ли автор (`mirror.profile`), потом `ensure` и публикация. `Like` отвечает `changed` и `author_id`: шлюзу нужно, кому слать пуш. `Feed(viewer, page, size)` страницами с единицы. `Comment`, `Trends(limit)` (свежий пересчёт, 0 значит `MAX_TRENDS`), `GetPost` (публичный пост), `DeletePost`. Ошибки user-service переводятся через `explain`: недоступен это `unavailable` с «сервис пользователей», чужой internal не пересказывается.
   - `x-request-id` входящего вызова уходит дальше во все вызовы user-service.

7. **auth-service**: вход по HTTP, токены по gRPC.
   - `run(http, grpc, Config, token)`: HTTP-сервер движка и gRPC `AuthServer` под `tokio::join!`, дочерний токен останавливает оба, если один кончился. `Config { users, journal, http, access_log, cors_origins }`, `HTTP_ADDR_ENV = "AUTH_HTTP_ADDR"`, `DEFAULT_HTTP_ADDR = "127.0.0.1:7881"`, `JOURNAL_ENV = "AUTH_JOURNAL"`, `CORS_ENV = "AUTH_CORS_ORIGINS"`. Ядра соцсети auth-service не знает.
   - `accounts/`: `Account { user_id, username }` (serde), `Accounts::open(path)` читает журнал NDJSON, одна учётная запись на строку. Оборванная последняя строка (нет `\n` в конце) при открытии отрезается `set_len`, чтобы следующая запись легла целой строкой. `add` сначала пишет строку и `flush`, потом кладёт в память. `find(ник)` без учёта регистра, `get(id)`.
   - `tokens/`: `Tokens` переезжают из шлюза и хранят `Account`, `TOKEN_BYTES`.
   - `http/`: `POST /auth/register` (тело `Credentials`, вызов `CreateProfile`, учётная запись в журнал, 201 `UserView`; ошибки user-service через `explain`, недоступен это 503), `POST /auth/login` (200 `Login` и cookie `session` как в m27, неизвестный ник 401), `POST /auth/logout` (защищён `bearer_auth`, 204 и стёртая cookie), `GET /health`. Слои `cors`, `access_log`, `token_from_cookie`, `bearer_auth`.
   - `grpc/`: `AuthService::new(accounts, tokens)`, `ValidateToken` (неизвестный токен: UNAUTHENTICATED с `x-error-code: unauthorized`), `GetAccount` (нет такой записи: NOT_FOUND).

8. **api-gateway** больше не держит ядро.
   - `Config` теряет `journal` и получает `services: Services { auth, users, posts }` (адреса gRPC, `Default` из `proto`, `from_env` из окружения). `Error`: `Upstream { service, source }`, `Bind`, `Io`.
   - `upstream/`: `Upstreams::connect(&services)` создаёт три канала `connect_lazy` один раз на весь шлюз; `auth()`, `users()` (с поднятыми лимитами `MESSAGE_MAX`), `posts()` отдают клиентов поверх клона канала. `failure(&status, service, rid) -> ErrorBody` через `explain`, internal ещё и в stderr с `rid`.
   - `auth/`: `RemoteAuth::check(token, rid) -> Result<Option<UserView>, ErrorBody>`: UNAUTHENTICATED это `Ok(None)`, остальное это `Err` (сервис входа лёг). `/ws` отвечает на `None` 401, на `Err` 503; HTTP-слой `bearer_auth_async` с `PROTECTED = ["/users/me", "/users/me/avatar"]` делает то же. Маршрутов `/auth/*` в шлюзе нет.
   - `chat/`: `ChatStore` с `Chats` под замком, `send` и `history`.
   - `routes/`: `Ctx::new(upstreams, registry, chats)`, `for_request()` (копия с новым `x-request-id`, её получает каждый WS-запрос), `request_id()`, `traced(msg)`, `users()`, `posts()`, `auth()`, `chats()`, `push(to, &push)`, `failed(service)`. Обработчики по файлам: `users.rs` (`find`, `follow`, `mute`), `posts.rs` (`create`, `like`, `delete`, `feed`), `chat.rs` (`send`, `history`). Пуши шлёт шлюз сам и только после успеха: `Followed`, если подписка изменилась; `Liked`, если лайк изменился и это не свой пост; `NewPost` каждому из `audience` автора; `Message` получателю. Перед перепиской шлюз спрашивает `GetProfile` собеседника. Обработчики не открывают соединений: никакого `connect(` в `routes/`.
   - HTTP: `/users/me`, `/users/:username`, `/users/me/avatar` (PUT и DELETE), `/avatars/:id`, `/posts/:id` идут в сервисы. `media/`, `http/auth.rs` и `routes/domain.rs` удаляются. `Registry` остаётся, но `impl Notifier` ему больше не нужен: `deliver` зовут обработчики.
   - `main.rs` печатает адреса сервисов.

9. **tools/social-cli** приходит новой версией: `run(gateway, auth, input, output)`, вход и выход идут в auth-service (`AUTH_HTTP_ADDR` или `--auth`), остальное в шлюз; `wait_ready` ждёт оба.

## Дерево после вехи

```
backend/
  Cargo.toml                          + tonic, prost, tokio-stream, новые крейты
  Makefile.toml  Makefile             + run-all, demo на четыре процесса
  .env.example                        + адреса auth, users, posts
  src/crates/
    proto/                            новый
      Cargo.toml  build.rs
      proto/social/v1/                common.proto auth.proto users.proto posts.proto
      src/lib.rs                      pub mod social::v1
      src/endpoints/mod.rs            AUTH, USERS, POSTS, AVATAR_MAX, MESSAGE_MAX
      src/status/mod.rs               domain, explain, traced, x-request-id
      tests/journey/                  main.rs m30_grpc_split.rs
    social-core/src/
      ports/repo.rs                   + Relations, UserRepo::relations
      network/                        + relations, following, hidden_by
      app/queries.rs                  + relations
    http-engine/src/middleware/       + bearer_auth_async
    test-support/src/slow/users.rs    + relations
  src/microservices/
    user-service/                     новый
      src/main.rs lib.rs
      src/config/ error/ service/ avatars/
      tests/journey/
    post-service/                     новый
      src/main.rs lib.rs
      src/config/ error/ service/ mirror/
      tests/journey/
    auth-service/                     новый
      src/main.rs lib.rs
      src/config/ error/ accounts/ tokens/ grpc/ state/
      src/http/mod.rs auth.rs
      tests/journey/
    api-gateway/src/
      auth/mod.rs                     RemoteAuth
      upstream/mod.rs                 Upstreams, failure
      chat/mod.rs                     ChatStore
      routes/mod.rs users.rs posts.rs chat.rs
      http/mod.rs users.rs posts.rs avatars.rs
      media/  http/auth.rs  routes/domain.rs   удалены
  tools/social-cli/                   клиент курса (новая версия)
```

Остальное дерево не меняется.

## Приёмка

- `api-gateway/tests/journey/m30_grpc_split.rs`: `/auth/*` в шлюзе нет, auth-service регистрирует, профиль появился в user-service, `ValidateToken` и `GetAccount` по gRPC; три сокета и десятки вызовов идут по одному TCP-соединению на каждый сервис (счётчик на прокси перед сервисами); один WS-запрос это один `x-request-id` в шлюзе и в post-service, у следующего свой; internal сервиса не уходит клиенту, доменная ошибка проходит с кодом и текстом; упавший post-service это `unavailable` и 503, упавший auth-service это 503 на защищённом пути и на `/ws`; аватар 5 МиБ проходит через шлюз, а клиент со стандартным лимитом его не читает; у каждого сервиса свой журнал, после перезапуска всё на месте; пуши только после успеха и только если что-то изменилось. Фейки сервисов в `fakes.rs`;
- `user-service`, `post-service`, `auth-service`: свои `tests/journey/m30_grpc_split.rs` (профили страницами, коды ошибок, связи и audience, большие аватары; автор проверяется в user-service, лента видит подписки и скрытия, лайк называет автора, тренды, журнал с зеркалом, недоступный user-service; HTTP входа, токены по gRPC, журнал учётных записей с оборванным хвостом, 503 при лежащем user-service);
- `proto`, `http-engine`, `social-core`: `tests/journey/m30_grpc_split.rs` (таблица кодов и `explain`, `x-request-id`, адреса; `bearer_auth_async`; `relations` и `Chats`);
- новые версии `support.rs` (весь стек в процессе на `127.0.0.1:0`, `Journals`, `auth_http`), `m24`, `m25`, `m26`, `m27`, `m29` шлюза: вход через auth-service, журналы сервисов;
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву, новые пакеты в workspace, `build.rs` с `tonic_prost_build` и `include_proto!("social.v1")`, у каждого сервиса `TcpListener::bind` в main.rs и `pub async fn run` в lib.rs, свой журнал в своей переменной, `serve_with_incoming_shutdown`, поднятый лимит в user-service и в клиенте шлюза, `connect_lazy` в `upstream/` и никакого `connect(` в `routes/`, ни `App`, ни `MemNetwork` в шлюзе, `/auth/` только в auth-service, `validate_token` в `auth/` шлюза, пуши `Followed`, `Liked` и по `audience` в обработчиках, `bearer_auth_async`, зависимости по слоям (ядро без tonic, auth-service без ядра), `run-all` с `parallel = true`, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

`demo` поднимает четыре процесса и клиента. В клиенте `register alice`, `login alice`, `post привет из четырёх процессов`. Во втором терминале `cargo run -p social-cli`, там `register bob`, `login bob`, `follow alice`, `feed`, `like 1`: у alice появятся пуши. Чтобы гонять сервисы руками, `cargo make run-all` в одном терминале (Ctrl-C гасит все) и клиент в другом. С журналами: раскомментируй `USERS_JOURNAL`, `POSTS_JOURNAL` и `AUTH_JOURNAL` в `.env`, останови всё по Ctrl-C и подними снова, alice и её пост на месте.
