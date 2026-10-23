# Веха 28. WebSocket вместо голого TCP

Голый TCP-порт с конвертами построчно хорош для своего клиента, но браузер на такой порт не постучится: из страницы можно открыть только HTTP и WebSocket. Поэтому NDJSON-порт уходит, а те же конверты теперь ходят текстовыми фреймами WebSocket по `GET /ws` на том же HTTP-порту, где живут вход, профили и аватары. Таблица методов, сессия, push во все окна: всё прежнее работает, меняется только транспорт.

WebSocket пишем сами, крейтом `websocket-engine`: рукопожатие с Sec-WebSocket-Accept, фреймы с маской и фрагментацией, ping и pong, закрытие с кодами, проверка UTF-8 и лимит сообщения. Чтобы рукопожатие встало поверх нашего HTTP, http-engine учится отдавать сокет вместе с уже прочитанными байтами после ответа 101.

## Что сделать

1. **Корень.** В `[workspace.dependencies]` путь `websocket-engine` и `sha1 = "0.10"`. В `.env.example` и `.env` строки `GATEWAY_ADDR` больше нет.

2. **http-engine, Upgrade** (`upgrade/`): `Upgraded { stream: TcpStream, rest: Vec<u8> }`, `UpgradeFuture`, `UpgradeHook`, поле `Response::upgrade` и `Response::on_upgrade(f)`. Сервер, получив от обработчика 101 с крючком, пишет голову ответа и отдаёт крючку сокет и всё, что уже лежало в буфере после запроса (первый фрейм мог прийти вместе с рукопожатием). Запись ответов вынесена в `server/write.rs`.

3. **`src/crates/websocket-engine`** (зависимости `tokio`, `bytes`, `sha1`, `http-engine`, в `[dev-dependencies]` `minifuzz`; без tungstenite, tokio-tungstenite, base64 и tokio-util; без комментариев; папки `base64/`, `frame/`, `assembler/`, `close/`, `stream/`, `handshake/`, `client/`; крейт растёт из проекта главы 10 nws_engine).
   - `base64_encode`, `base64_decode` (строгий: длина, алфавит, `=` только в конце, нулевой хвост).
   - `accept_key` по RFC 6455, `check_upgrade(&request)` (GET и HTTP/1.1, `Upgrade: websocket`, `Connection` со списком токенов, один ключ на 16 байт, версия 13, иначе 400, 405 с `Allow` или 426 с `Sec-WebSocket-Version: 13`), `WsConfig { allowed_origins, allow_missing_origin, max_message, close_timeout }`, `upgrade(&request, &config, on_connect)` с проверкой Origin (чужой и `null` это 403).
   - Фреймы: `Frame`, `Opcode`, `Role`, `Decoder` (маска обязательна от клиента и запрещена от сервера, кратчайшая длина, зарезервированные биты, управляющие фреймы целые и до 125 байт, лимит), `encode_frame`, `MaskGen`, `ProtoError` с `close_code` (1002, 1007, 1009).
   - `Assembler`: фрагменты в сообщение, ping посреди фрагментов, UTF-8 проверяется по мере сборки (битый байт это 1007 сразу, эмодзи, разрезанный по байтам, это нормальный текст).
   - `close`: `parse_close`, `encode_close`, `valid_close_code`, `Closing` с фазами.
   - `WsStream<S>`: `recv` (сам отвечает pong, закрывается по Close с эхом кода), `send`, `ping`, `close(code, reason)` с ожиданием ответа не дольше `close_timeout`, `close_code`; `WsError`.
   - Клиент: `connect(addr, host, path, origin, config)`, `client_request`, `new_key`, `verify_response` (101, Accept, никаких расширений и подпротоколов, о которых не просили); фреймы клиента маскируются свежим ключом.

4. **api-gateway.**
   - NDJSON-порт и всё, что его обслуживало, уходят: папки `server/` и `connection/` удаляются (`run` больше не принимает TCP-слушатель конвертов: `run(http, config, shutdown)`, `main.rs` занимает один HTTP-порт). Общее состояние переезжает в `state/` (`Shared` с `sessions: Arc<Semaphore>` и токеном остановки).
   - `GET /ws` (`ws/`): сначала `check_upgrade`, потом токен из `?token=` (чужой это 401 обычным HTTP-ответом, без 101; без токена сокет анонимный и представляется `session.hello`, как раньше), потом место в `sessions` (`max_connections`; лишний клиент ждёт ответа 101), потом `upgrade` с `WsConfig { allowed_origins: cors_origins, allow_missing_origin: true, max_message: max_line, .. }`.
   - Сессия: текстовый фрейм это конверт, ответ уходит текстовым фреймом с тем же `id`, push из `Hub` тоже; двоичный фрейм это закрытие с `UNSUPPORTED` (1003); сообщение больше `max_line` это 1009; простой дольше `idle_timeout` и остановка шлюза это закрытие с `GOING_AWAY` (1001).
   - Свой `tests/http_api.rs` поправь под новый `run`.

5. **tools/social-cli** приходит новой версией: свой маленький WebSocket-клиент (`social_cli::Ws`, `accept_for`), `run(addr, input, output)` с одним адресом HTTP-порта, флаг `--addr` и `GATEWAY_HTTP_ADDR`.

## Дерево после вехи

```
backend/
  Cargo.toml                        + websocket-engine, sha1
  .env.example  .env                - GATEWAY_ADDR
  src/crates/
    http-engine/src/
      upgrade/mod.rs                Upgraded, UpgradeHook
      server/mod.rs  write.rs       101 отдаёт сокет крючку
      response/mod.rs               + upgrade, on_upgrade
    websocket-engine/
      Cargo.toml
      src/lib.rs  base64/  frame/  assembler/  close/  stream/  handshake/  client/
      tests/journey/
  src/microservices/api-gateway/src/
    main.rs                         один HTTP-порт
    lib.rs                          run(http, config, shutdown)
    state/mod.rs                    Shared
    ws/mod.rs                       GET /ws, сессия
    server/  connection/            удалены
  tools/social-cli/                 клиент курса (новая версия, WebSocket)
```

Остальное дерево не меняется.

## Приёмка

- `websocket-engine/tests/journey/m28_websocket.rs`: base64 и Accept по RFC, рукопожатие и все отказы, Origin, свой клиент со своим сервером, первый фрейм вместе с рукопожатием, немаскированные ответы сервера, фрагменты с ping, эмодзи по байтам, pong без участия приложения, нарушения протокола (1002), UTF-8 (1007), лимит (1009), закрытие с обеих сторон и молчаливый собеседник, клиент: запрос, свежая маска, байты после 101, плохие ответы; 30 клиентов сразу; HTTP рядом с сокетами; кодек на случайных нарезках (minifuzz::Rng);
- `api-gateway/tests/journey/m28_websocket.rs`: токен в URL представляет сокет, чужой токен это 401 до 101; рукопожатие по RFC и защита от чужих страниц; всё прежнее поверх фреймов; двоичный фрейм это 1003; новые версии `support.rs`, `m24_tcp_gateway.rs` (лимит, простой, 1009 и 1001 поверх WebSocket), `m26_http_hardening.rs` и `m27_browser_media.rs` под новый клиент;
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву, `server/` и `connection/` шлюза удалены, `LinesCodec` в шлюзе больше нет, websocket-engine без чужих WebSocket-крейтов и без комментариев, GUID в рукопожатии, Origin, `MaskRequired`, 1009, `InvalidUtf8` при сборке, `Opcode::Pong` в потоке, `rest` у `Upgraded`, маршрут `/ws`, `check_upgrade` и `acquire_owned` в сессии, 1001 при остановке, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

Клиент подключается к `ws://127.0.0.1:7880/ws`, и всё прежнее (`register`, `login`, `post`, `feed`, `msg`, `avatar`) работает поверх фреймов. В журнале шлюза первой строкой `GET /ws 101`.
