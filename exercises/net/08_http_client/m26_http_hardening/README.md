# Веха 26. HTTP для настоящего сервиса

Первый HTTP соцсети работает, пока к нему ходят вежливо: один запрос, ответ, закрыли. Настоящие клиенты держат соединение и шлют запросы друг за другом, браузер ждёт `100 Continue`, а злой клиент присылает заголовки по байту в секунду и держит сокет вечно. Пора довести http-engine до боевого: keep-alive и конвейер, дедлайны против slowloris, лимиты, middleware, мягкая остановка. И раз сервер стал взрослым, ему нужен свой клиент: крейт `http-client`, которым ты сам пишешь интеграционные тесты HTTP-API шлюза.

Первым настоящим потребителем middleware станет `/users/me`: шлюз узнаёт пользователя по `Authorization: Bearer <токен>` из `/auth/login`, а без токена честно отвечает 401.

## Что сделать

1. **http-engine, сервер** (`server/`, этот крейт растёт из проекта главы 07 nhr_engine).
   - `ServerConfig { limits, idle_timeout 60 с, head_timeout 10 с, body_timeout 30 с, max_connections 1024, drain_timeout 30 с }`; `Server::new(router)`, `Server::with_handler(handler)`, `.config(cfg)`, `.limits(limits)`, `.serve(listener, shutdown)`.
   - Keep-alive: в HTTP/1.1 соединение живёт, пока клиент не попросил `Connection: close`; HTTP/1.0 закрывается, если не просил `keep-alive` (тогда ответ подтверждает `Connection: keep-alive`). Конвейер: запросы из одного read обслуживаются по очереди, ответы строго по порядку, остаток буфера переходит к следующему запросу.
   - Простой между запросами дольше `idle_timeout` тихо закрывает соединение. Начатая голова запроса обязана прийти целиком за `head_timeout` с первого байта (общий дедлайн через `sleep_until`, а не таймаут на каждый read), тело за `body_timeout`, иначе ответ 408 и закрытие.
   - Лимиты отвечают до чтения тела: 431 на огромные заголовки, 413 на большой Content-Length и большой chunk; `Expect: 100-continue` получает `HTTP/1.1 100 Continue`, если тело будет принято, а иначе сразу отказ; незнакомое `Expect` это 417.
   - `Response::streaming(status, content_type, Receiver<Vec<u8>>)`: тело неизвестной длины идёт chunked (пустые куски пропускаются), для HTTP/1.0 как есть до закрытия.
   - Лимит соединений на `Semaphore`. Мягкая остановка: новые соединения не принимаются, простаивающие keep-alive закрываются, запрос в работе получает полный ответ с `Connection: close`, а через `drain_timeout` опоздавшие обрываются. Ответ без потока это одна запись: голова и тело одним буфером. Паника обработчика это 500, и соединение остаётся рабочим.
   - `parse_head` и `Head { request, head_len, length, chunked }` с `expects_continue` и `has_unknown_expectation` теперь публичные. `Router::into_handler`.

2. **http-engine, middleware** (`middleware/`): `Middleware = Arc<dyn Fn(Handler) -> Handler + Send + Sync>`, `middleware(f)`, `apply(handler, &layers)` (первый слой самый внешний), `access_log(sink)` пишет строку `GET /path 200 3ms` после ответа, `bearer_auth(verify, guard)`: сначала затирает пришедший от клиента `X-User`, затем по `Authorization: Bearer <токен>` (схема и имя без учёта регистра) спрашивает `verify(token) -> Option<String>` и кладёт ответ в `X-User`; запрос, для которого `guard(&request)` истинно, без годного токена получает 401 и `WWW-Authenticate: Bearer`.

3. **`src/crates/http-client`** (зависимости `tokio`, `serde`, `serde_json`; без hyper, reqwest, httparse; без комментариев; папки `url/`, `wire/` с `head.rs` и `body.rs`, `pool/`, `client/` с `request.rs`, `error/`; крейт растёт из проекта главы 08 nhc_client).
   - `parse_url`, `resolve` (относительный Location), `Url { host, port, path }`, `UrlError`, `RedirectError`.
   - Конец тела ответа по правилам: HEAD, 204, 304 и 1xx без тела, Content-Length, chunked, до закрытия; разные Content-Length и тело больше `max_body` это `Error::Protocol`.
   - `Client::new(Config { connect_timeout, total_timeout, max_redirects, max_idle_per_host, idle_ttl, max_body })`: `get`, `send(Request)`, `get_json`, `post_json`, `idle_connections()`; пул keep-alive по (хост, порт), в пул возвращаются только дочитанные соединения; протухшее соединение повторяется один раз и только для идемпотентного метода; общий дедлайн на весь запрос; редиректы 301, 302 и 303 становятся GET без тела, 307 и 308 сохраняют метод и тело, лимит переходов, Authorization и Cookie не уходят на чужой хост.
   - `Error { Url, Request, ConnectTimeout, TotalTimeout, Connect, Io, Protocol, TooManyRedirects, BadRedirect, Status { code, body }, Decode }` с Display по-русски; `Error::status()`.

4. **api-gateway.**
   - HTTP-порт обслуживает `Server::with_handler(http::app(..))` с `Config::http: ServerConfig`. Слои шлюза: `access_log` (если `Config::access_log`, строки в stderr с префиксом `api-gateway:`; `main.rs` его включает) и `bearer_auth` с проверкой токена в `Tokens`; под защитой `PROTECTED = ["/users/me"]`.
   - `GET /users/me` → 200 `UserView` владельца токена, без токена 401 с `WWW-Authenticate: Bearer`. Статичный `/users/me` побеждает `/users/:username`.
   - **Свои тесты** `src/microservices/api-gateway/tests/http_api.rs`: шлюз в процессе на `127.0.0.1:0` и твой `http-client` (dev-зависимость шлюза): регистрация, вход, `/users/me` с Bearer и без, конфликт ника, всё по одному keep-alive соединению. Тесты курса этот файл не трогают, они только требуют, чтобы он был и ходил в шлюз через `http_client`.

5. **tools/social-cli** приходит новой версией: команда `me` (GET /users/me с токеном из `login`), а все HTTP-запросы клиент шлёт по одному keep-alive соединению (`social_cli::Web`, счётчик `opened()`).

## Дерево после вехи

```
backend/
  Cargo.toml                        + http-client
  src/crates/
    http-engine/src/
      lib.rs                        + Middleware, ServerConfig, Head, parse_head
      server/mod.rs                 keep-alive, дедлайны, лимиты, поток, остановка
      middleware/mod.rs             apply, access_log, bearer_auth
      parse/mod.rs                  + pub Head, parse_head
      response/mod.rs               + stream, streaming
      router/mod.rs                 + into_handler
    http-client/
      Cargo.toml
      src/lib.rs  url/  pool/  error/
      src/wire/mod.rs  head.rs  body.rs
      src/client/mod.rs  request.rs
      tests/journey/
  src/microservices/api-gateway/
    Cargo.toml                      + http-client в dev-dependencies
    src/http/mod.rs                 app со слоями, /users/me
    src/config/mod.rs               + http, access_log
    tests/http_api.rs               твои интеграционные тесты
  tools/social-cli/                 клиент курса (новая версия)
```

Остальное дерево не меняется.

## Приёмка

- `http-engine/tests/journey/m26_http_hardening.rs`: keep-alive и HEAD без лишних байт, HTTP/1.0, конвейер по порядку с половинкой запроса в остатке, простой, slowloris получает 408 по общему дедлайну, зависшее тело 408, 431 и 413 заранее, chunked-тело и лимит куска, 100-continue и 417, chunked-ответ кусками и сырой поток для 1.0, паника и жизнь соединения, порядок слоёв и Bearer с затиранием X-User, журнал с временем, лимит соединений, мягкая остановка; новая версия `m25_http_api.rs` просит `Connection: close` явно;
- `http-client/tests/journey/m26_http_hardening.rs`: JSON туда и обратно, ошибки по видам, пул, ответы без тела, chunked и до закрытия, мусор и лимит тела, редиректы и их правила, учётные данные не уезжают, петля редиректов, общий дедлайн, повтор протухшего соединения только для GET;
- `api-gateway/tests/journey/m26_http_hardening.rs`: `/users/me` по Bearer и подделка X-User, клиент курса держит одно соединение, пул `http-client` держит одно соединение со шлюзом, остановка закрывает keep-alive, команда `me`;
- твой `api-gateway/tests/http_api.rs` есть и зелёный; тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву, общий дедлайн головы, 100 Continue, Semaphore, слои и `WWW-Authenticate` в движке, затирание `x-user`, пул и повтор только идемпотентного в клиенте, `bearer_auth` и `access_log` в шлюзе, `http_client` в своих тестах, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

В клиенте: `register alice`, `login alice`, `me`. В окне шлюза появляются строки журнала `api-gateway: GET /users/me 200 0ms`, а `curl -i http://127.0.0.1:7880/users/me` без токена получает 401 с `WWW-Authenticate: Bearer`.
