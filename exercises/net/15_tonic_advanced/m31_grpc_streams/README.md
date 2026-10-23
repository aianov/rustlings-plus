# Веха 31. gRPC в бою

После прошлой вехи сервисы разговаривают, но наивно. Шлюз сам кладёт `x-request-id` в каждый вызов и забывает про него в половине мест; медленный user-service держит WebSocket-запрос сколько угодно; кто действует, едет в теле запроса, и любой, кто дотянулся до порта post-service, может постить от чужого имени; переписка всё ещё живёт в памяти шлюза; а шлюз понятия не имеет, поднялся ли сосед. Эта веха собирает уроки главы 15: перехватчики, срок как бюджет, отмена, потоки, здоровье и мягкая остановка.

## Что сделать

1. **Корень.** В `[workspace.dependencies]`: `tonic-health = "=0.14.6"`, `tower = { version = "0.5", default-features = false }`, `http = "1"`, пути `grpc-kit` и `chat-service`. Заготовка `cargo xtask new-service chat-service`. `.env.example` (и `.env`): `CHAT_GRPC_ADDR=127.0.0.1:50054`, в комментарии `CHAT_JOURNAL`. `Makefile.toml`: задача `chat-service`, она же в `run-all` и `demo`; в `Makefile` то же.

2. **proto.** `chat.proto` и новые версии `users.proto`, `posts.proto` приходят готовыми.
   - Кто действует, теперь едет в метаданных `x-user-id`, а не в теле: в `PairRequest`, `PutAvatarRequest`, `CreatePostRequest`, `PostAction`, `FeedRequest`, `CommentRequest` поле действующего удалено, его номер и имя `reserved`. `DeleteAvatar` берёт пустой `DeleteAvatarRequest`.
   - `Posts.Watch(WatchRequest{recent}) returns (stream FeedEvent)`, `FeedEvent { oneof kind { Post history; CaughtUp caught_up; Post live } }`.
   - `Chat`: `Send(SendRequest{peer_id, text}) → Message{from_id, to_id, text}`, `History(HistoryRequest{peer_id, limit}) → HistoryReply`, `Subscribe(SubscribeRequest{}) returns (stream Message)`.
   - `endpoints/`: `CHAT` (`CHAT_GRPC_ADDR`, `127.0.0.1:50054`).

3. **src/crates/grpc-kit** (новый крейт: `http`, `proto`, `service-runtime`, `tokio`, `tonic`, `tonic-health`, `tower`; ядра не знает).
   - `stamp/`: `Stamp` это перехватчик клиента. `Stamp::new(rid)`, `fresh()` (новый `x-request-id`), `.user(id)`, `.until(instant)`, `.within(budget)`, `.endless()` (снять срок, для потоков), `follow(&request)` (из входящего вызова: `x-request-id`, пользователь из `Caller` или `x-user-id`, срок из `grpc-timeout`), `request_id()`, `user_id()`, `left()`. В `call` ставит `x-request-id`, `x-user-id` и `request.set_timeout(остаток)`; если остаток ноль, вызов не уходит в сеть: `DEADLINE_EXCEEDED`.
   - `caller/`: `USER_ID = "x-user-id"`, `Caller(u64)`, перехватчик сервера `caller` (нет заголовка: дальше без пользователя; не число: `INVALID_ARGUMENT`; число: `Caller` в extensions), `caller_of(&request) -> Result<u64, Status>` (нет `Caller`: `UNAUTHENTICATED` с `x-error-code: unauthorized`).
   - `budget/`: `parse_grpc_timeout` (до 8 цифр и единица H, M, S, m, u, n) и `budget_of(&request)`.
   - `access/`: tower-слой `AccessLog::new(service, Option<LogSink>)` пишет `Access { service, request_id, user, path, code, elapsed }` на каждый вызов, код из `grpc-status` ответа; `Display`: `post-service [rid] u=1 /social.v1.Posts/CreatePost 0 2ms`. `LogSink::new(f)`, `LogSink::stderr()`.
   - `health/`: `drain::<S>(reporter, shutdown, grace)` (по сигналу сразу NOT_SERVING, пауза `grace`, потом сервер закрывается), `bounded(serving, shutdown, limit)` (жёсткий предел мягкой остановки), `HARD_STOP = 2 с`, `Upstream { label, service, channel }` и `Readiness::watch(upstreams, &stop)`: по задаче на соседа слушает поток `Health/Watch` (не опрос `Check`), на NOT_SERVING или обрыве отпускает поток и переподключается с паузой от 50 мс до 1 с. `missing()`, `is_ready()`, `wait(limit)`.

4. **Сервисы** (user, post, chat, auth): `Config` получает `grace` (100 мс) и `log: Option<LogSink>` (main ставит `LogSink::stderr()` и `grace` 1 с). Сервер: `.layer(AccessLog::new(..))`, `health_reporter` с SERVING, основной сервис за `InterceptedService::new(svc, caller)` (у auth-service перехватчик не нужен), `serve_with_incoming_shutdown` с `drain`, всё под `bounded(.., grace + HARD_STOP)`.
   - Действующий берётся `caller_of`. Запись, которая начата для клиента, доделывается в `tokio::spawn`, даже если клиент ушёл по сроку: регистрация, подписка, скрытие, публикация, лайк, удаление, комментарий, сообщение.
   - post-service ходит в user-service клиентом с `Stamp::follow(&request)`: тот же `x-request-id`, тот же пользователь и остаток срока. `Watch`: синхронизирует зеркало читателя (под своим сроком `SYNC_BUDGET = 1 с`), `App::watch(читатель, recent)` (0 значит 20, не больше `RECENT_MAX = 100`) в задаче с `mpsc`, задача кончается, когда клиент закрыл поток или сервис останавливается.
   - auth-service зовёт `CreateProfile` клиентом с `Stamp::fresh()`.

5. **social-core**: `Chats::check(from, to, text) -> Result<String, DomainError>` проверяет сообщение, ничего не записывая; `send` проверяет им же.

6. **chat-service**: `run(listener, Config { users, journal, grace, log }, token)`, `JOURNAL_ENV = "CHAT_JOURNAL"`.
   - `store/`: `Store::open(path)` поднимает переписку из NDJSON (`Line { from, to, text }`, оборванный хвост отрезается), `send` сначала `Chats::check`, потом строка в журнал с `flush`, потом память; `Refused { Domain, Journal }`.
   - `hub/`: `Hub` держит подписки пользователей (`mpsc` на `BACKLOG = 64`), `subscribe`, `deliver` (полная или брошенная подписка выкидывается), `subscribers`, `close` (на остановке закрывает все потоки; новая подписка после этого: `UNAVAILABLE`).
   - `service/`: `Send` спрашивает user-service, есть ли собеседник, потом в `tokio::spawn` пишет и раздаёт подписчикам получателя; `History` (`limit` 0 значит 20, не больше 100); `Subscribe` для `x-user-id`.

7. **contracts**: `FeedPush { FeedPost { to, post, live }, FeedCaughtUp { to } }` (тег `event` как у `Push`) и `live/`: `LIVE_METHODS = ["posts.watch", "posts.unwatch"]`, `LiveCall`, `decode_live` (`recent` по умолчанию 20, от 1 до 100).

8. **api-gateway** без ядра.
   - `Config`: `services.chat`, `budget` (`BUDGET = 300 мс`). `Upstreams` отдаёт клиентов с перехватчиком (`auth(&stamp)`, `users`, `posts`, `chat`) и `readiness(&stop)`.
   - `routes/ctx.rs`: `Ctx::new(upstreams, registry, budget)`, `.on(outbox)` (контекст соединения со своим `Watching`), `for_request(user)` (`Stamp::fresh().user(..).within(budget)`), `stamp()`, `users()`, `posts()`, `chat()`, `auth()`, `endless_posts()`, `outbox()`, `watching()`, `push`, `failed`.
   - `chat.send` и `chat.history` идут в chat-service; пуш о сообщении приходит из `Subscribe`: `inbox/` (`Inboxes::join(user)` открывает один поток на пользователя на первом устройстве и ждёт его до 101, `InboxGuard` закрывает его, когда уходит последнее устройство; поток оборвался: переподключение с паузой).
   - `posts.watch` (`recent`) открывает `Watch` без срока и шлёт в сокет `FeedPush`, `posts.unwatch` его закрывает, сокет закрылся: тоже. Один просмотр на сокет.
   - `GET /health`: 200, когда все сервисы SERVING, иначе 503 `unavailable` с «жду сервисы: ...».
   - Ошибка контракта собирается без ядра (`report` в `error/`).

9. **tools/social-cli** приходит новой версией: `feed --live [сколько]` и `feed --stop`.

## Дерево после вехи

```
backend/
  src/crates/
    proto/proto/social/v1/            chat.proto, новые users.proto и posts.proto
    grpc-kit/                         новый: stamp/ caller/ budget/ access/ health/
    contracts/src/live/               LIVE_METHODS, decode_live
    contracts/src/push/               + FeedPush
    social-core/src/chat/             + Chats::check
  src/microservices/
    chat-service/                     новый: config/ error/ store/ hub/ service/
    user-service/ post-service/ auth-service/   + здоровье, журнал, caller, мягкая остановка
    api-gateway/src/
      inbox/mod.rs                    подписки на сообщения
      routes/ctx.rs live.rs           контекст запроса, живая лента
      chat/                           удалён
  tools/social-cli/                   клиент курса (новая версия)
```

## Приёмка

- `api-gateway/tests/journey/m31_grpc_streams.rs`: сообщение через chat-service приходит на оба устройства; один поток Subscribe на пользователя, открыт до 101, закрыт с последним устройством (фейк chat-service считает потоки); медленный user-service: ответ по бюджету 300 мс, до user-service доехали остаток срока и `x-user-id`; `/health` называет сервис без SERVING; живая лента: история, отметка, новый пост подписки, чужой не приходит, `posts.unwatch`; строки журнала двух сервисов про один запрос с одним `x-request-id`; клиент курса с `feed --live`;
- `grpc-kit`, `chat-service`, `post-service`, `contracts`, `social-core`, `proto`: свои `m31_grpc_streams.rs` (штамп и срок, `caller`, `grpc-timeout`, журнал вызовов, готовность по Watch и NOT_SERVING до остановки; переписка, подписки, журнал; поток `Watch` и его закрытие на остановке; `decode_live` и `FeedPush`; `Chats::check`; chat.proto и зарезервированные поля);
- новые версии тестов m30 (действующий в `x-user-id`), `support.rs` (chat-service в стеке, ожидание готовности, `logged`, события ленты), `fakes.rs`, `m29` шлюза (таблица с живыми методами) и `m09` xtask (заготовка проверяется на `sandbox-service`, ведь `chat-service` теперь настоящий);
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву, `grpc-kit` и `chat-service` в workspace, шлюз без `social-core`, у сервисов `AccessLog`, `health_reporter`, `caller`, `bounded` и `tokio::spawn` для записей, `Stamp::follow` в post-service, `set_timeout` в штампе, готовность через `Health/Watch` без `Check`, `Readiness::watch` в шлюзе, `inboxes.join` до 101, `Drop` у `InboxGuard`, `endless()` для потока ленты, никакого ручного `traced(` в шлюзе, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

В клиенте `register alice`, `register bob`, `login bob`, `follow alice`, `feed --live`. Во втором окне `cargo run -p social-cli`, `login alice`, `post привет`: пост появится у bob сам. `msg bob привет` придёт во все окна bob. В выводе сервисов видно строки журнала с одним `x-request-id` на запрос; останови всё Ctrl-C и посмотри, как сервисы сначала говорят NOT_SERVING.
