# Веха 24. Соцсеть слушает порт

До сих пор с соцсетью говорил один человек за одним терминалом, а все пользователи жили в одном процессе с ним. Пора открыть дверь: шлюз `api-gateway` занимает TCP-порт, и каждый, кто подключился, говорит с тем же ядром. Конверты те же, что в m13, только теперь они идут по сети, построчно, а соединение само представляется ником. Личное сообщение или новая подписка прилетают push сразу во все окна пользователя.

TCP не знает, где кончается одно сообщение: для него это поток байтов, который режется как попало. Поэтому сначала свой крейт `framing` с декодером над буфером и `Framed`, а уже на нём шлюз. Терминал `tools/repl` уходит, вместо него клиент курса `tools/social-cli`: его кладёт курс, ты его не правишь.

## Что сделать

1. **Корень.** В `members` три маски: `src/crates/*`, `src/microservices/*`, `tools/*`. В `[workspace.dependencies]` появляются `bytes = "1"`, `framing` и `social-cli` (пути), у `tokio` фичи `net` и `signal`. В `.env.example` и `.env`: `SERVICE=api-gateway` и `GATEWAY_ADDR=127.0.0.1:7878`. В `Makefile` `SERVICE ?= api-gateway` и цель `demo`, в `Makefile.toml` задача `[tasks.demo]`: собрать шлюз и клиент, запустить шлюз в фоне, клиент в терминале, а на выходе погасить шлюз. Папка `tools/repl` удаляется целиком.

2. **`src/crates/framing`** (зависит только от `tokio` и `bytes`, без комментариев; папки `codec/`, `framed/`, `lines/`, `length/`).
   - `trait Decoder { type Item; type Error: From<io::Error>; fn decode(&mut self, &mut BytesMut) -> Result<Option<Item>, Error>; fn decode_eof(..) }`: `decode_eof` по умолчанию отдаёт остаток, а если в буфере осталась половина сообщения, это `io::ErrorKind::UnexpectedEof`.
   - `trait Encoder<Item> { type Error: From<io::Error>; fn encode(&mut self, Item, &mut BytesMut) -> Result<(), Error> }`.
   - `Framed<S, C>`: `new`, `recv().await -> Result<Option<Item>, Error>` (читает в свой буфер, пока декодер не отдаст сообщение; `None` это чистый конец потока), `send(item).await` (одно сообщение одним `write_all`), `flush`, `close` (shutdown), `get_ref`, `codec`, `buffered`, `into_parts`. `recv` с AsyncRead, `send` с AsyncWrite, поэтому `Framed` работает и с половинками сокета.
   - `LinesCodec::new(max_len)`: строки до `\n` (и `\r\n`) в `String`; длиннее `max_len` это `LinesError::TooLong { max }`, причём сразу, как только в буфере больше `max_len + 1` байт без `\n`; не UTF-8 это `BadUtf8` (испорченная строка из буфера вынута); `Encoder<String>` и `Encoder<&str>` пишут строку и `\n`, длинную отказываются писать.
   - `LengthCodec::new(max_len)`: кадр это длина `u32` big-endian и тело, `Item = Bytes`; длина больше лимита это `LengthError::TooLong { len, max }` сразу по заголовку, до ожидания тела; кодеры для `&[u8]`, `Bytes`, `Vec<u8>`.
   - `LinesError` и `LengthError` с Display по-русски и `source`.

3. **contracts.**
   - `session/`: `SESSION_METHODS = ["session.hello", "session.bye"]`, `ANONYMOUS_METHODS = ["users.register", "users.find"]`, `SessionCall { Hello { username }, Bye }`, `decode_session(&Envelope)` (неизвестный метод, чужой сервис и плохие данные как у `decode_call`), `needs_actor(method)`: всем методам, кроме анонимных и сессионных, нужна сессия.
   - `Envelope::act_as(user)`: если в `data` нет `user`, вписывает `"u_N"`; если есть и совпадает, оставляет; если чужой, это `Error::Forbidden { method, claimed }` с кодом `forbidden`; `data` не объект это `bad_data`.
   - `encode(&T) -> Result<String>`: одна строка JSON без перевода строки (пара к `decode`).

4. **`src/microservices/api-gateway`** (`main.rs`, `lib.rs`, папки `config/`, `server/`, `connection/`, `protocol/`, `session/`, `hub/`, `error/`; без комментариев; JSON только через contracts).
   - `main.rs`: `TcpListener::bind` прямо здесь (адрес из `GATEWAY_ADDR`, по умолчанию `DEFAULT_ADDR`), печатает адрес, Ctrl-C отменяет токен, `fn main` возвращает `ExitCode`.
   - `lib.rs`: `run(listener, Config, CancellationToken).await -> Result<()>` поднимает ядро (с журналом из `Config::journal`, если он задан), принимает соединения, а после отмены даёт им `drain` на завершение и закрывает ядро. Тесты поднимают шлюз в процессе на `127.0.0.1:0`.
   - `Config { max_connections 256, idle_timeout 300 с, max_line 64 КиБ, accept_pause 100 мс, outbox 64, drain 5 с, journal: None }`, `Config::from_env()` берёт журнал из `SOCIAL_JOURNAL`.
   - `server/`: цикл accept с лимитом на `Semaphore` (разрешение берётся до accept и живёт вместе с соединением, лишние клиенты ждут в очереди ядра ОС), временная ошибка accept это пауза `accept_pause`, а не падение; задачи соединений в `TaskTracker`, мягкая остановка токеном.
   - `connection/`: `into_split`, чтение `Framed` с `LinesCodec::new(max_line)` под `timeout(idle_timeout, ..)`, ответы и push пишет отдельная writer-задача из ограниченной очереди `outbox`. Пустая строка пропускается; не JSON это ответ с ошибкой и `id: null`, соединение живёт; слишком длинная строка это ответ `too_long` и закрытие соединения; простой дольше `idle_timeout` и отмена тоже закрывают его, но ответ на уже прочитанный запрос уходит.
   - `protocol/` и `session/`: автомат `Session { Anonymous, Introduced(UserView) }`. `session.hello {username}` представляет соединение (пока на честном слове, без пароля): неизвестный ник `not_found`, второй hello без bye `conflict`, ответ `UserView`. `session.bye` возвращает `Anonymous` и отвечает тем, кто вышел; без сессии `unauthorized`. Метод, которому нужен актёр, без сессии это `unauthorized`, с сессией `act_as` подставляет `user`. Дальше `decode_call` и `serve` ядра, ответ `Response` с тем же `id`.
   - `hub/`: `Hub` (реестр соединений по пользователю) это `Notifier` ядра: push кладётся через `try_send` в очередь каждого соединения пользователя, переполненная очередь не тормозит ядро (`dropped()` считает потери). `Hub::join` отдаёт `Link`, а `Drop` у `Link` убирает соединение из реестра.

5. **`tools/social-cli`** приходит с инъекцией: член воркспейса, свои версии зависимостей. `cargo run -p social-cli` подключается к `GATEWAY_ADDR`; `help` перечисляет команды (`register`, `hello`, `bye`, `find`, `follow`, `mute`, `post`, `like`, `delete`, `feed`, `msg`, `history`). Тесты шлюза подключают его как dev-зависимость: `social-cli.workspace = true`.

## Дерево после вехи

```
backend/
  Cargo.toml                        members + src/microservices/*; bytes, framing, social-cli; tokio + net, signal
  Makefile  Makefile.toml           SERVICE api-gateway, demo
  .env.example  .env               SERVICE=api-gateway, GATEWAY_ADDR
  src/crates/
    framing/
      Cargo.toml
      src/lib.rs  codec/mod.rs  framed/mod.rs  lines/mod.rs  length/mod.rs
      tests/journey/                тесты курса
    contracts/src/
      session/mod.rs                SessionCall, decode_session, needs_actor
      envelope/mod.rs               + act_as, encode
      error/mod.rs                  + Forbidden
  src/microservices/api-gateway/
    Cargo.toml
    src/
      main.rs                       TcpListener::bind
      lib.rs                        run
      config/mod.rs  server/mod.rs  connection/mod.rs  protocol/mod.rs
      session/mod.rs  hub/mod.rs  error/mod.rs
    tests/journey/                  тесты курса
  tools/
    social-cli/                     клиент курса (инъекция)
    xtask/
  tools/repl                        удалён
```

Остальное дерево не меняется.

## Приёмка

- `framing/tests/journey/m24_tcp_gateway.rs`: декодер без ввода-вывода хранит остаток; строки и кадры собираются при любой нарезке потока; обрыв посреди сообщения это `UnexpectedEof`; длинная строка и огромная длина отвергаются сразу; испорченный UTF-8 не ломает следующую строку; `send` пишет по сообщению;
- `contracts/tests/journey/m24_tcp_gateway.rs`: методы сессии, анонимные методы, `act_as`, `encode`;
- `api-gateway/tests/journey/m24_tcp_gateway.rs`: hello и действия от своего имени; правила сессии (`unauthorized`, `not_found`, `conflict`, `forbidden`); push во все соединения пользователя; мусор и слишком длинные строки; простой; лимит соединений; мягкая остановка и журнал между запусками; клиент курса регистрирует, постит, подписывается и пишет, а второе окно видит push;
- новые версии `xtask/tests/journey/m09_dev_tooling.rs` (сервис по умолчанию api-gateway) и `social-core/tests/journey/m16_test_support.rs` (релизный бинарь теперь шлюз); тесты терминала ушли вместе с ним; `cargo make check` зелёный.

Архитектура: файлы по дереву, `TcpListener::bind` в `main.rs`, `framing` только на `tokio` и `bytes`, `Semaphore`, `into_split`, `timeout` и `LinesCodec` в шлюзе, без `read_line`, `unbounded_channel` и `serde_json` в шлюзе, `tools/repl` нет, цели `demo`, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

В клиенте: `register alice`, `hello alice`, `post Привет, сеть!`, `feed`. Во втором терминале `cargo run -q -p social-cli`, там `register bob`, `hello bob`, `follow alice`, `msg alice как ты?`, и в первом окне сразу появляются строки `* на тебя подписался ...` и `* ...: как ты?`.
