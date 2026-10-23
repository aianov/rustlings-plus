# Веха 25. Первый HTTP

Соцсеть слушает порт, но говорит на языке, который понимает только наш клиент. Браузер, curl, мобильное приложение и соседний сервис ждут HTTP. И «представиться ником на честном слове» годилось, пока в сети были только свои: теперь вход идёт по HTTP и выдаёт токен, а TCP-соединение предъявляет его в `session.hello`. Пароля пока нет, но угадать чужой токен уже нельзя: 128 бит из генератора ОС.

HTTP пишем сами, крейтом `http-engine`: инкрементальный парсер с лимитами, роутер с параметрами пути, async-обработчики в одной таблице. Парсеры ломаются на том, чего ты не придумал, поэтому рядом свой фаззер `minifuzz`, и парсер проходит разрез входа во всех точках, свойство «собрали и разобрали обратно» и мутационный прогон без единой паники.

## Что сделать

1. **Корень.** В `[workspace.dependencies]` пути `http-engine` и `minifuzz`, крейт `getrandom = "0.3"` (выбор по правилам главы cargo_deps: маленький, без лишних зависимостей, берёт байты у ОС). В `.env.example` и `.env` строка `GATEWAY_HTTP_ADDR=127.0.0.1:7880`.

2. **`src/crates/http-engine`** (зависимости `tokio`, `serde`, `serde_json`, в `[dev-dependencies]` `minifuzz`; без hyper, axum, httparse, tokio-util; без комментариев; папки `method/`, `status/`, `headers/`, `request/`, `response/`, `parse/` с `chunked.rs` и `text.rs`, `router/`, `server/`).
   - `Method` (FromStr, `as_str`, Display), `StatusCode` (`new` только для 100..=599, `reason`, `allows_body`), `Headers` (имена без учёта регистра, несколько значений, порядок), `Request { method, target, version, headers, body, params }` (`raw_path`, `path`, `param`, `query` с раскодированием после разбиения по `&` и `=`, `keep_alive`, `json::<T>()`: не `application/json` в UTF-8 это 415, кривое тело 400), `Response { status, headers, body }` (`new`, `text`, `json`, `error` с телом `{"error": "..."}`, `header` отвергает `\r\n` в значении, `to_bytes(head_only)` сам ставит `Content-Length`).
   - `parse_request(buf, &Limits) -> Result<Parsed, ParseError>`: `Parsed::Partial`, пока запрос не пришёл целиком, `Parsed::Complete { request, consumed }`; `Limits { max_head_bytes 8192, max_headers 64, max_body 1 МиБ }`; Host обязателен в HTTP/1.1 и один; тело по Content-Length или chunked; два разных Content-Length или CL вместе с TE это `Smuggling`; `ParseError::status()` даёт 400, 413, 431, 501, 505.
   - `Router`: `route(method, "/users/:id", handler)`, `get`, `post`; статичный сегмент побеждает параметр, HEAD отвечает маршрутом GET, неизвестный путь 404, чужой метод 405 с `Allow`. Обработчик это `Handler = Arc<dyn Fn(Request) -> BoxFuture<Response> + Send + Sync>`, `handler(async fn)` и `with_state(Arc<S>, async fn(Arc<S>, Request))`.
   - `Server::new(router).limits(..).read_timeout(..).serve(listener, shutdown).await`: по запросу на соединение с `Connection: close` (keep-alive в следующей вехе), паника обработчика это 500, а сервер живёт дальше; после `shutdown` новые соединения не принимаются, начатые дорабатывают.

3. **`src/crates/minifuzz`** (без зависимостей, без комментариев; папки `rng/`, `mutate/`, `shrink/`, `fuzz/`, `split/`): `Rng` (splitmix64: `next_u64`, `byte`, `below`, `index`, `range`, `chance`, `pick -> Option`), `mutate`, `shrink`, `fuzz(&Config, corpus, target) -> Option<Crash>` (catch_unwind, бюджет итераций и времени, сжатие падающего входа), `input_for` (повтор входа по seed и номеру итерации), `run_property`, `check_splits`.

4. **contracts.** `auth/`: `Credentials { username }`, `Login { token, user: UserView }`, `TOKEN_LEN = 32`, `is_token`. `SessionCall::Hello { token }`: `session.hello` принимает только `{"token": "..."}` из 32 строчных шестнадцатеричных знаков, всё другое `bad_data`.

5. **api-gateway.** `run(tcp, http, config, shutdown)` слушает два порта, `main.rs` занимает оба (`GATEWAY_ADDR` и `GATEWAY_HTTP_ADDR`, по умолчанию `DEFAULT_HTTP_ADDR`). `auth/`: `Tokens` (токен → пользователь, `issue` берёт 16 байт из `getrandom` и пишет их hex), `TOKEN_BYTES`. `http/`: маршруты на `http-engine`, ошибки телом `{"error": {"code", "message"}}` с кодами как у TCP и статусом по коду (`bad_data` 400, `unauthorized` 401, `forbidden` 403, `not_found` 404, `conflict` 409, `rejected` 422):
   - `POST /auth/register {"username"}` → 201 `UserView`, занятый ник 409;
   - `POST /auth/login {"username"}` (пока без пароля) → 200 `Login`, неизвестный ник 401;
   - `GET /users/:username` → 200 `UserView` или 404, кривой ник 400;
   - `GET /posts/:id` (`7` или `p_7`) → 200 `PostView` только публичного и не удалённого поста (новый запрос ядра `App::public_post` через порт `PostRepo::public_post`), иначе 404, не id 400;
   - `GET /health` → 200 `ok`.
   `session.hello {token}` по TCP ищет токен в `Tokens`: чужой или устаревший это `unauthorized`. Регистрация и поиск по TCP остаются анонимными.

6. **tools/social-cli** приходит новой версией: `register` и `login` ходят по HTTP, `login` сам кладёт токен в `session.hello`, новые команды `profile <ник>` и `show <id>`; адрес HTTP из `GATEWAY_HTTP_ADDR` или `--http`.

## Дерево после вехи

```
backend/
  Cargo.toml                        + http-engine, minifuzz, getrandom
  .env.example  .env                + GATEWAY_HTTP_ADDR
  src/crates/
    http-engine/
      Cargo.toml
      src/lib.rs  method/  status/  headers/  request/  response/  router/  server/
      src/parse/mod.rs  chunked.rs  text.rs
      tests/journey/
    minifuzz/
      Cargo.toml
      src/lib.rs  rng/  mutate/  shrink/  fuzz/  split/
      tests/journey/
    contracts/src/
      auth/mod.rs                   Credentials, Login, is_token
      session/mod.rs                Hello { token }
    social-core/src/                + public_post (network/posts.rs, ports/repo.rs, storage/mem/repo.rs, app/queries.rs)
    test-support/src/slow/posts.rs  + public_post
  src/microservices/api-gateway/src/
    main.rs                         два TcpListener::bind
    lib.rs                          run(tcp, http, config, shutdown)
    auth/mod.rs                     Tokens
    http/mod.rs                     маршруты
    config/mod.rs                   + HTTP_ADDR_ENV, DEFAULT_HTTP_ADDR
    server/mod.rs  protocol/mod.rs  + токены
  src/microservices/api-gateway/tests/journey/
    support.rs                      общий запуск шлюза, TCP и HTTP в тестах
  tools/social-cli/                 клиент курса (новая версия)
```

Остальное дерево не меняется.

## Приёмка

- `http-engine/tests/journey/m25_http_api.rs`: инкрементальный разбор и счёт байт; отказы (Host, контрабанда, лимиты, неизвестный метод); chunked и percent-decoding; точные байты ответа и защита от `\r\n` в заголовке; любой разрез потока даёт тот же запрос (`check_splits`); сгенерированные запросы разбираются обратно (`run_property`); 20 000 мутаций без паники (`fuzz`); minifuzz только в dev-зависимостях; сервер: параметры, state, статичный сегмент, HEAD, 404, 405 с Allow, JSON, 415, 400, паника 500 и жизнь после неё;
- `minifuzz/tests/journey/m25_http_api.rs`: splitmix64, границы, детерминированная мутация, сжатие, фаззер находит, сжимает и повторяет подложенный баг, бюджеты, первый падающий seed, split-checker;
- `contracts/tests/journey/m25_http_api.rs` и новая версия `m24_tcp_gateway.rs`: вход и токены на проводе, hello только по токену;
- `api-gateway/tests/journey/m25_http_api.rs`: регистрация и вход, токены не повторяются, профиль и пост по ссылке, health, HEAD, 405, порт закрыт после остановки; новая версия `m24_tcp_gateway.rs` на токенах и общий `support.rs`;
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву, свой HTTP без hyper, axum, httparse и tokio-util, minifuzz без зависимостей, без комментариев в обоих крейтах, токены только из getrandom, маршруты в http/mod.rs, hello без ника, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

В клиенте: `register alice`, `login alice`, `post Привет, HTTP!`, `show 1`, `profile alice`. Снаружи тоже работает: `curl -i http://127.0.0.1:7880/posts/1`, `curl -i -X POST -H 'Content-Type: application/json' -d '{"username":"bob"}' http://127.0.0.1:7880/auth/register`.
