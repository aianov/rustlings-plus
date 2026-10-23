# Веха 27. HTTP для браузера и файлов

К соцсети собирается фронт: страница на `http://localhost:5173` хочет звать API шлюза на `7880`, а браузер просто так не даст ей прочитать ответ с чужого origin. Браузер же хранит вход: токен в cookie, которую скрипт страницы не видит. И у профиля наконец появится лицо: аватар, который загружают раз, а показывают тысячи раз, поэтому он отдаётся с ETag и кусками по Range.

Всё нужное для этого уже лежит в главе 09: слой CORS, cookie, условные запросы и диапазоны. Здесь они переезжают в http-engine и встают в шлюз.

## Что сделать

1. **http-engine** (крейт растёт из проекта главы 09 nha_api_kit; новые папки `cors/`, `cookie/`, `conditional/`, `body/`).
   - `cors/`: `CorsPolicy { origins, methods, headers, expose_headers, credentials, max_age }` и слой `cors(policy)`: предзапрос OPTIONS с `Access-Control-Request-Method` отвечает сам (204 с Allow-Origin, Allow-Methods, Allow-Headers, Max-Age, при credentials Allow-Credentials, или 403), остальные ответы, и ошибки тоже, получают `Access-Control-Allow-Origin` для разрешённого origin и всегда `Vary: Origin`; `"*"` вместе с credentials становится эхом origin.
   - `cookie/`: `parse_cookies`, `cookie`, `request_cookie(&request, name)`, `SetCookie { name, value, max_age, path, domain, http_only, secure, same_site }` с `to_header()` и проверками (`SameSite=None` без `Secure`, префиксы `__Host-` и `__Secure-`), `SetCookie::delete`, `SameSite`, `CookieError`; слой `token_from_cookie(name)`: если `Authorization` нет, а cookie есть, её значение становится `Authorization: Bearer ...`.
   - `conditional/`: `etag(bytes)` (FNV-1a в кавычках), `etag_matches`, `parse_range` и `RangeSpec { Full, Partial { start, end }, Unsatisfiable }`, `serve_bytes(&request, content_type, bytes)`: If-None-Match раньше диапазона (304 без тела), Range только для GET и HEAD (206 с Content-Range, 416 с `bytes */длина`), Accept-Ranges, ETag, `Cache-Control: no-cache`.
   - `body/`: `BodyReader::new(reader, buf, Framing, limit)` читает тело кусками (Content-Length и chunked) с лимитом, `drain(cap)` дочитывает недочитанное.

2. **api-gateway.**
   - `Config` получает `cors_origins` (из `GATEWAY_CORS_ORIGINS` через запятую, в `.env.example` `http://localhost:5173`) и `avatar_max` (256 КиБ). Слои HTTP по порядку: `cors(policy)` снаружи всех (иначе предзапрос без токена получит 401), `access_log`, `token_from_cookie("session")`, `bearer_auth`. Политика: методы GET, HEAD, POST, PUT, DELETE; заголовки Content-Type, Authorization, If-None-Match, Range; наружу ETag, Content-Range, Accept-Ranges; credentials; Max-Age 600.
   - `POST /auth/login` ещё и ставит `Set-Cookie: session=<токен>; Max-Age=86400; Path=/; HttpOnly; SameSite=Lax`. `POST /auth/logout` (под токеном или cookie) отзывает токен (`Tokens::revoke`) и стирает cookie (`Max-Age=0`), ответ 204; после него токен не годится ни по HTTP, ни для `session.hello`.
   - Профиль (`GET /users/:username` и `/users/me`) отдаётся с ETag и `Cache-Control: no-cache`; совпавший If-None-Match это 304 без тела.
   - Аватары в памяти шлюза (`media/`: `Avatars`, `Avatar { content_type, bytes }`): `PUT /users/me/avatar` под токеном, только `image/*` (иначе 415), больше `avatar_max` это 413, а keep-alive соединение остаётся рабочим (тело дочитано сервером); 201 при первой загрузке, 204 при замене, ответ с ETag. `GET /avatars/:id` (`u_1` или `1`) через `serve_bytes`: ETag, 304, Range 206 и 416, HEAD; чужой id 404. `DELETE /users/me/avatar` → 204 или 404.
   - Маршруты разложены: `http/mod.rs` (слои, роутер, коды), `http/auth.rs`, `http/users.rs`, `http/posts.rs`, `http/avatars.rs`.

3. **tools/social-cli** приходит новой версией: `logout` (отзыв токена и выход из TCP-сессии), `avatar set <файл>` (png, jpg, gif, webp), `avatar get <ник> [файл]` (если файл уже есть, докачивает хвост по `Range: bytes=<длина>-`).

## Дерево после вехи

```
backend/
  .env.example  .env                + GATEWAY_CORS_ORIGINS
  src/crates/http-engine/src/
    lib.rs                          + CorsPolicy, cors, SetCookie, serve_bytes, BodyReader, ...
    cors/mod.rs  cookie/mod.rs  conditional/mod.rs  body/mod.rs
  src/microservices/api-gateway/src/
    lib.rs                          + Avatars
    config/mod.rs                   + cors_origins, avatar_max, CORS_ENV
    auth/mod.rs                     + revoke
    media/mod.rs                    Avatars
    http/mod.rs  auth.rs  users.rs  posts.rs  avatars.rs
  tools/social-cli/                 клиент курса (новая версия)
```

Остальное дерево не меняется.

## Приёмка

- `http-engine/tests/journey/m27_browser_media.rs`: cookie туда и обратно и запреты; CORS на предзапросе и на всех ответах, wildcard с credentials; условные ответы и диапазоны; BodyReader кусками, лимиты и обрыв; cookie сессии становится Bearer;
- `api-gateway/tests/journey/m27_browser_media.rs`: CORS только для своего фронта и на ошибках тоже, предзапрос к закрытому адресу без 401; вход ставит HttpOnly SameSite=Lax cookie, по ней открывается `/users/me`, выход стирает её и отзывает токен; профиль отвечает 304; аватары: 401, 415, 413 без поломки соединения, 201 и 204, ETag и 304, Range 206 и 416, HEAD, 404, удаление; клиент курса загружает аватар и докачивает обрезанный файл;
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву, `Vary` и предзапрос в cors, HttpOnly и проверка SameSite=None в cookie, 416 и If-None-Match в conditional, BodyReader, CORS первым слоем шлюза, `token_from_cookie`, `http_only: true` и `SameSite::Lax` у cookie сессии, `SetCookie::delete` при выходе, `serve_bytes` у аватаров, origin из `GATEWAY_CORS_ORIGINS`, никаких `"*"` в шлюзе, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

В клиенте: `register alice`, `login alice`, `avatar set me.png`, `avatar get alice`. Оборви файл (`truncate -s 100 alice.avatar`) и повтори `avatar get alice`: клиент скачает только хвост. Снаружи: `curl -i -H 'Origin: http://localhost:5173' http://127.0.0.1:7880/health` показывает `Access-Control-Allow-Origin`.
