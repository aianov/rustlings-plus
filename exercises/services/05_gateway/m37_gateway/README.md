# Веха 37. Одна дверь

У соцсети до сих пор два входа: клиент знает адрес шлюза для сокета и отдельный адрес auth-service для регистрации и входа. Внутри шлюза таблица методов пропускает в обработчик сырой конверт, и каждый обработчик сам разбирает `data` и сам помнит, что пользователь мог не войти. Срок токена шлюз проверяет один раз, при подключении: сокет с токеном на пятнадцать минут живёт хоть сутки. Запросы одного соединения ждут в очереди за лимитом, а не получают отказ. Экран профиля клиент собирает тремя запросами по очереди.

Эта веха делает шлюз единственной дверью. Маршруты типизированы, и защищённый обработчик получает `Principal`, а не `Option`. Дубликат маршрута роняет старт. Срок токена сверяется на каждом кадре, и по нему соединение закрывается само. Лимит запросов в полёте отказывает сразу. Внутренние metadata строятся только из проверенного контекста. Карточка профиля собирается параллельно с частичным ответом. А `/auth/*` проксируется в auth-service, и клиенту хватает одного адреса.

Всё собирается из главы 05_gateway: упражнения с svgw1 по svgw15, проект svgw_grpc и квиз svgw_quiz. Кадр, маршрут, сессия, metadata, агрегация и прокси там уже были, здесь они встают в настоящий шлюз.

## Что сделать

1. **contracts**: код `exhausted` (429, временный) в каталоге. Конверт `Envelope` получает необязательный `meta` (карта строк, пустая на провод не пишется) и `with_meta`; `Response` получает необязательный `request_id`, `Response::new` и `with_request_id`. `read_data::<T>(method, data)` разбирает `data` в тип: отказ это `bad_data` с текстом «data не подходит к методу …» и деталью только про недостающее или лишнее поле, текст serde в ответ не идёт. `to_data` и реэкспорт `Map`, `Value`. Вид `ProfileView { user, posts: Option<Vec<PostView>>, stats: Option<StatsView>, partial }` (недошедшая часть пишется `null`), `StatsView { followers, following }`, вызов `users.profile` (`Call::Profile { target }`).

2. **proto**: `Account.expires_at` (секунды Unix; заполняет только `ValidateToken`), `rpc AuthorPosts(AuthorPostsRequest { author_id, limit }) returns (FeedReply)`, адрес `AUTH_HTTP` (`AUTH_HTTP_ADDR`, `127.0.0.1:7881`), `is_request_id`; в `status` код `Exhausted` это `ResourceExhausted`.

3. **grpc-kit**: белый список клиентских metadata. `CLIENT_KEYS = ["x-trace-id", "accept-language"]`, `client_value` (1..=128 байт видимого ASCII), `Stamp::pass(key, value)` берёт только ключи из списка (имя приводится к нижнему регистру, значение проверяется, повтор заменяет прежнее), перехватчик кладёт их в вызов, `Stamp::follow` несёт их дальше.

4. **http-engine**: `Request.peer` (адрес соединения). `ServerConfig.proxy_protocol`: сервер за своим TLS-краем первой строкой соединения ждёт `PROXY TCP4|TCP6 <клиент> <сервер> <порт клиента> <порт сервера>\r\n` или `PROXY UNKNOWN\r\n` и берёт адрес клиента из неё, без строки соединение закрывается. Разбор `parse_proxy_line` (`ProxyLine::{Incomplete, Broken, Unknown, Peer}`, не длиннее `PROXY_LINE_MAX = 107`) и запись `proxy_line` в модуле `proxy/`.

5. **tls-edge**: `terminate_announced` (как `terminate`, но первой пишет движку строку PROXY с адресом клиента), `mutual_server_config(cert, key, ca)` (пускает только клиентов с сертификатом нашего CA) и `mutual_client_config(ca, cert, key)`.

6. **http-client**: схема `https` (порт 443 по умолчанию) поверх `tokio-rustls`: `Config.tls` (настройки rustls) и `Config.tls_name` (имя в сертификате, когда в адресе IP). `https` без `tls` это ошибка адреса, а не открытый текст; редирект со сменой схемы отвергается. `Error::Tls`. `Client::relay(request)`: один обмен без перехода по редиректам, для прокси. Соединение `conn/` (`Conn::{Plain, Tls}`), ключ пула учитывает схему. В `[dependencies]` добавляется `tokio-rustls`, а тестам вехи нужны `certgen`, `service-runtime` и `tls-edge` в `[dev-dependencies]`.

7. **auth-service**: `ValidateToken` отдаёт `expires_at`. HTTP-ответы берут номер запроса из `x-request-id` шлюза, если он годен (`is_request_id`), иначе свой. HTTP входа слушает за взаимным TLS: его вызывает только шлюз.

8. **post-service**: `PostService::by_author(author, limit)` (новые первыми, без удалённых), `dto::author_page` (`limit` от 1 до `MAX_AUTHOR_POSTS = 50`), контроллер `author_posts`.

9. **api-gateway**.
   - `common/principal.rs`: `Principal { id, username, role, expires_at }` (`view`, `left`, `is_expired`), `Visitor { Guest, Member(Principal) }` (`role`, `user`), `unix_now`. `common/error.rs`: `Duplicates(Vec<String>)` и `Error::Routes`.
   - `dto/data.rs`: типы данных запросов (`FindUser`, `Target`, `NewPost`, `PostRef`, `EditPost`, `FeedAsk`, `Letter`, `HistoryAsk`, `WatchAsk`, `Nothing`) со значениями по умолчанию.
   - `routes/mod.rs`: `Routes` строит таблицу. `route(name, |ctx, data: Req| ...)` и `route_authed(name, |ctx, principal, data: Req| ...)`: обёртка разбирает `data` через `read_data` и собирает ответ через `to_data`. Защищённый маршрут для гостя это `unauthorized` до разбора данных, свой `user` в `data` по-прежнему сверяется (`act_as`). Повтор имени не заменяет маршрут, а запоминается: `build()` отдаёт `Router` или `Duplicates` со всеми задвоенными именами по разу в порядке появления, и шлюз с такой таблицей не стартует. `Router::{names, is_authed, dispatch}`. Маршруты по файлам `routes/users.rs`, `posts.rs`, `chat.rs`, `live.rs`; нигде в `routes/` нет `Option<Principal>`.
   - `users.profile { target }` (открытый): `GetProfile` обязателен, `AuthorPosts` и `Relations` необязательны, все три идут одновременно (`tokio::join!`), необязательная часть ограничена бюджетом `part_budget` и при сбое или опоздании становится `null` с именем в `partial` (`posts`, `stats`).
   - `routes/ctx.rs`: `Ctx::for_request(&visitor, &meta)`: свежий `x-request-id`, бюджет запроса, `x-user-id` и `x-user-role` только из `Visitor` (гость едет с ролью `guest`), клиентские metadata только через `Stamp::pass`. `parts(..)` и `part_budget()`.
   - `controller/ws.rs`: токен из `Authorization: Bearer` или `?token=`; без токена сокет гостевой, два разных токена это 400 `bad_data`, просроченный это 401 `expired`. Гость не попадает в реестр рассылок и не открывает входящие.
   - `controller/work.rs`: запросы соединения в `JoinSet`, словарь `task::Id → id кадра`, семафор на `max_in_flight`: разрешение берётся `try_acquire_owned`, без места ответ `exhausted` со своим `id` сразу. Паника обработчика это `internal` с номером запроса, текст паники только в журнал.
   - `controller/conn.rs`: срок токена превращается в монотонный дедлайн; кадр после срока и сам дедлайн закрывают соединение. Перед закрытием начатые запросы доделываются (не дольше `GRACE`), их ответы и `{"id": null, "error": {"code": "expired"}}` уходят клиенту, если сокет жив, затем закрытие кодом 4001 (`EXPIRED`). При остановке шлюза начатые запросы тоже отвечены до 1001. Каждый ответ несёт `request_id`.
   - `controller/proxy.rs`: `/auth/*` уходит в auth-service тем же путём со строкой запроса (auth-service сам живёт под `/auth` и ставит cookie с `Path=/auth`). До апстрима: путь с `..`, `//`, `\` или их кодами это 404, тело больше `GATEWAY_PROXY_BODY_MAX` это 413. Не уходят ни туда, ни обратно заголовки уровня соединения и перечисленные в `Connection`; присланные клиентом `X-Forwarded-For`, `X-Real-IP`, `Forwarded`, `X-Forwarded-Proto`, `X-Forwarded-Host`, `X-User*`, `x-internal-*` и `x-request-id` выбрасываются, взамен свои `X-Forwarded-For` и `X-Real-IP` из адреса соединения, `X-Forwarded-Proto` и номер запроса шлюза. Апстрим недоступен: 502 `unavailable`, не ответил за `GATEWAY_PROXY_TIMEOUT_MS`: 504 `timeout`, тела пишет шлюз.
   - `client/upstream.rs`: `AuthHttp` на http-client (`relay`, без редиректов, по взаимному TLS с именем `auth-service`, когда у шлюза есть сертификаты). `client/auth.rs`: `RemoteAuth::check -> Option<Principal>`, истёкший по `expires_at` это `expired`.
   - Свой TLS-край шлюза `terminate_announced`, движок с `proxy_protocol`: адрес клиента настоящий и за TLS.
   - `http-client` переезжает из `[dev-dependencies]` шлюза в `[dependencies]`: теперь он нужен прокси, а не только тестам.
   - Конфиг: `services.auth_http` (`AUTH_HTTP_ADDR`), `GATEWAY_MAX_IN_FLIGHT` (8), `GATEWAY_PART_BUDGET_MS` (150), `GATEWAY_PROXY_TIMEOUT_MS` (5000), `GATEWAY_PROXY_BODY_MAX` (16 КиБ); нули отвергаются при старте. Те же ключи в `.env.example` и `.env`.

10. **tools/social-cli** приходит новой версией: один адрес (`--auth` больше нет), регистрация, вход и обновление токена идут через шлюз. До входа клиент открывает гостевой сокет: `find` и новая команда `card <ник>` (карточка: подписчики, подписки, последние посты, что не успело прийти) работают без входа, а действие от имени пользователя отвечает «сначала войди». На закрытие кодом 4001 и на ответ `expired` клиент сам обновляет токен по cookie `refresh`.

## Дерево после вехи

```
backend/src/
  crates/http-engine/src/proxy/                 строка PROXY v1, адрес клиента
  crates/http-client/src/conn/                  соединение TCP или TLS
  microservices/api-gateway/src/
    common/principal.rs                         Principal, Visitor
    dto/data.rs                                 типы data для маршрутов
    controller/work.rs                          запросы в полёте, лимит, паники
    controller/proxy.rs                         обратный прокси /auth/*
    client/upstream.rs                          AuthHttp на http-client
tools/social-cli/                               один адрес, гостевой сокет, card (новая версия)
```

## Приёмка

- `api-gateway/tests/journey/m37_gateway.rs`: дубликаты называются все и роняют старт; `bad_data` называет поле и не цитирует присланное; гость ищет и смотрит карточки, а защищённый метод отказывает до разбора `data`; два разных токена это 400; истёкший токен: начатый запрос отвечен, затем `expired` без id и код 4001, повторное подключение 401 `expired`; лимит 2 даёт отказ `exhausted` третьему сразу, паника не съедает разрешение; остановка отвечает на начатое; metadata от фейкового user-service: пользователь и роль из токена, номер запроса шлюза, белый список с отсевом перевода строки и длинных значений, гость с ролью `guest`; карточка из трёх вызовов; части идут одновременно, опоздавшая часть это `null` в `partial`, без пользователя ответа нет; прокси чистит заголовки в обе стороны, сохраняет обе cookie, отвечает 404 и 413 без апстрима, 504 по таймауту, 502 без апстрима; клиент курса работает с одним адресом и сам продлевает сокет.
- `http-engine`: разбор строки PROXY и адрес клиента в `Request.peer`; `tls-edge`: край пишет строку PROXY, взаимный TLS пускает только своих; `http-client`: https с `tls_name`, `relay` отдаёт редирект как есть; `grpc-kit`: белый список и `Stamp::follow`; `contracts`: `read_data`, `meta`, `null` в карточке, код `exhausted`; `post-service`: посты автора и граница `limit`; `auth-service`: `expires_at` и номер запроса шлюза.
- новые версии тестов прошлых вех: `support.rs` и `fakes.rs` шлюза (регистрация и вход через шлюз, `auth_http`, срок в фейковом `ValidateToken`, metadata в `Seen`, `FakePosts`), сценарии клиента с одним адресом в m24, m26, m27, m29, m31, m32, m35, m36, гостевой сокет в m24 и m29, отказ `exhausted` вместо очереди и `users.profile` в m29, прокси `/auth/*` в m30, `auth_http` в m30, m31 и `http_api.rs`, типизированные маршруты в m29 и m35, m13 и m35 contracts (`users.profile`, `request_id`, `exhausted`), m26 http-engine (`proxy_protocol`);
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву; маршруты в `routes/users.rs`, `posts.rs`, `chat.rs`, нигде в `routes/` нет `Option<Principal>`; `build` и `Duplicates` в таблице, `try_acquire_owned`, `join_next_with_id` и `exhausted` в работе соединения, срок и 4001 в соединении, `tokio::join!` и `timeout` в карточке, hop-by-hop и `X-Forwarded-For` в прокси, `relay` в клиенте auth, `terminate_announced` в шлюзе, `mutual_server_config` в auth-service, `Stamp::pass` в контексте, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

Клиенту хватает одного адреса. До входа: `find alice` и `card alice` (гость), `post привет` ответит «сначала войди». Потом `register alice correct-horse`, `login alice correct-horse`, `post привет`, `card alice`. Чтобы увидеть закрытие по сроку, запусти стек с `AUTH_ACCESS_TTL_SECS=5 AUTH_CLOCK_LEEWAY_SECS=0 cargo make demo`: через пять секунд после входа клиент сам напишет «срок входа вышел» и «токен обновлён по refresh», не дожидаясь команды. Прокси видно и без клиента: `curl --cacert certs/ca.pem -i -X POST https://127.0.0.1:7880/auth/login -H 'Content-Type: application/json' -d '{"username":"alice","password":"correct-horse"}'` вернёт обе cookie, а прямой заход в auth-service на 7881 без сертификата шлюза закончится отказом TLS.
