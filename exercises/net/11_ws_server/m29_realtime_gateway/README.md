# Веха 29. Шлюз реального времени

После прошлой вехи конверты бегают фреймами WebSocket, но сессия всё ещё живёт по одному: медленный запрос держит все следующие, push ждёт, пока уйдёт ответ, а телефон, который въехал в тоннель и перестал читать, копит очередь, пока шлюз не упрётся в сокет. К тому же сокет открывается анонимно и занимает место, а кто это, выясняется только по `session.hello`.

Здесь шлюз собирается из решений главы 11: реестр соединений с уборкой через Drop, ограниченная очередь на каждое соединение с местом под Close, отключение медленного клиента кодом 1008, heartbeat, токен до ответа 101, таблица методов, параллельные запросы одного сокета с потолком и события `{event, data}` вперемешку с ответами.

## Что сделать

1. **contracts.**
   - `Push` на проводе теперь событие: `{"event": "followed", "data": {"to": "u_1", "by": "u_2"}}` (`#[serde(tag = "event", content = "data")]`). У события нет `id`, у ответа есть, по этому клиент их и различает.
   - `session/` уходит целиком (`SESSION_METHODS`, `ANONYMOUS_METHODS`, `SessionCall`, `decode_session`, `needs_actor`): сокет представлен с рукопожатия, а нужен ли методу пользователь, решает таблица шлюза.

2. **websocket-engine.** `WsStream::idle_for()`: сколько прошло с последних байт от собеседника. Весть это что угодно: сообщение, ping, pong.

3. **api-gateway.**
   - `auth/`: трейт `Authenticator { fn check(&self, token: &str) -> Option<UserView> }`, `Tokens` его реализует. `Shared` держит `auth: Arc<dyn Authenticator>`, через него проверяют токен и `/ws`, и `bearer_auth`.
   - `GET /ws` (`ws/mod.rs`): после `check_upgrade` токен ищется в `Authorization: Bearer` (туда же его кладёт `token_from_cookie`, поэтому браузер с cookie сессии открывает сокет без токена в адресе), потом в `?token=`. Нет токена или он чужой: 401 обычным ответом, без 101. Анонимных сокетов больше нет: `session.hello` и `session.bye` уходят, папки `session/` и `hub/` удаляются.
   - `registry/`: `Registry` знает соединения каждого пользователя. `register(user, outbox)` отдаёт `Registration`, её Drop убирает соединение (а с последним устройством и пользователя), как бы оно ни кончилось; `devices`, `users`, `connections`, `kicked`, `deliver(user, &push)`, `impl Notifier for Registry`. `Outbox` (`Outbox::channel(n)` или `registry.outbox(n)`): очередь `Out::Text` на `n` сообщений плюс одно место под `Out::Close`. На `n + 1`-м сообщении клиент отключается (`kick`): в очередь ложится `Out::Close(TOO_SLOW)` (1008), `kicked()` будит соединение, счётчик растёт один раз.
   - `routes/`: `Routes` с `.route(name, |ctx, envelope| ..)` (обработчику пользователь не нужен) и `.route_authed(name, |ctx, user, envelope| ..)` (пользователь приходит из сокета, а не из data); `names`, `is_authed`, `dispatch`, повторное имя это паника при старте. `Routes::social()`: `users.find` открыт, остальные восемь методов контракта идут через `route_authed` в ядро (`domain.rs`: `act_as`, `decode_call`, `serve`), `users.register` живёт только в HTTP. `Ctx` пока несёт `App`. `run(http, config, shutdown)` берёт `Routes::social()`, `run_with(http, config, routes, shutdown)` принимает свою таблицу.
   - `ws/conn.rs`: цикл соединения это один `select!` из остановки, очереди, чтения (только пока задач в полёте меньше `max_in_flight`), завершений `JoinSet`, срока молчания (`idle_timeout` минус `ws.idle_for()`) и тика `interval_at`, на котором уходит ping. Запись в сокет прерывается `kicked()` и остановкой, но начатый фрейм дописывается не дольше 2 секунд, иначе сокет бросается без Close. Каждый запрос это задача в `JoinSet`, а обработчик внутри неё ещё раз под `tokio::spawn`, чтобы паника стала ответом `internal` с тем же `id`. Ответы и события идут через ту же `Outbox`, по готовности.
   - Коды закрытия: 1001 при остановке и для молчуна, 1003 на двоичный фрейм, 1008 для медленного клиента, 1009 ставит сам `WsStream`.
   - `Config` получает `ping_every` (30 с) и `max_in_flight` (8), `idle_timeout` по умолчанию 90 с.

4. **tools/social-cli** приходит новой версией: стартует без сокета, `login` открывает `/ws?token=`, `logout` отзывает токен и закрывает сокет, события читает из `event` и `data`, а сокет, закрытый шлюзом, показывает с кодом.

## Дерево после вехи

```
backend/
  src/crates/
    contracts/src/
      push/mod.rs                   событие {event, data}
      session/                      удалён
    websocket-engine/src/stream/    + idle_for
  src/microservices/api-gateway/src/
    lib.rs                          run, run_with
    auth/mod.rs                     + Authenticator
    config/mod.rs                   + ping_every, max_in_flight
    registry/mod.rs                 Registry, Registration, Outbox
    routes/mod.rs  domain.rs        Routes, Ctx
    protocol/mod.rs                 разбор конверта и ответы
    state/mod.rs                    + registry, auth, routes
    ws/mod.rs  conn.rs              токен до 101, цикл соединения
    hub/  session/                  удалены
  tools/social-cli/                 клиент курса (новая версия)
```

Остальное дерево не меняется.

## Приёмка

- `api-gateway/tests/journey/m29_realtime_gateway.rs`: реестр убирает соединения сам, и при панике тоже; push уходит на каждое устройство в виде `{event, data}`; переполненная очередь отключает с 1008 ровно один раз; таблица покрывает контракт, повтор метода это паника; токен до 101 (нет, чужой, Bearer, cookie); события вперемешку с ответами; запросы одного сокета отвечают по готовности, паника обработчика это `internal`; лимит в полёте у каждого сокета свой; телефон, который не читает, отключается с 1008, а ноутбук и отправитель не ждут; heartbeat держит живых и закрывает молчунов с 1001; два окна одного пользователя в клиенте курса видят всё;
- `websocket-engine/tests/journey/m29_realtime_gateway.rs`: `idle_for` сбрасывают сообщение, ping и pong;
- новые версии `support.rs`, `m24_tcp_gateway.rs`, `m27_browser_media.rs`, `m28_websocket.rs` шлюза (сокет только с токеном) и `m13_wire_contracts.rs`, `m15_background_fanout.rs`, `m24_tcp_gateway.rs` контракта (событие вместо тега `push`, без `session/`);
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву, `routes/mod.rs` и `registry/mod.rs` на месте, `hub/` и `session/` шлюза и `session/` контракта удалены, `unbounded_channel` в шлюзе нет, `impl Notifier for Registry` и `impl Drop for Registration`, `.route_authed` в таблице и `act_as` в маршрутах, трейт `Authenticator` и `auth.check` до 101, 1008 в реестре, `kicked()`, `interval_at`, `JoinSet`, `max_in_flight` и `idle_for` в цикле соединения, `idle_for` в `WsStream`, событие с `tag = "event", content = "data"`, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

В клиенте `register alice` и `login alice`. Во втором терминале ещё одно окно (`cargo run -p social-cli`) и снова `login alice`, в третьем `register bob`, `login bob`, `follow alice`, `msg alice привет`: подписка и сообщение приходят в оба окна alice. Медленного клиента играет тест `slow_device_is_cut_off_and_nobody_else_waits`: телефон alice перестаёт читать, шлюз отключает его кодом 1008, а ноутбук получает каждое сообщение без задержки.
