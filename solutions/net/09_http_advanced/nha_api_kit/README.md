# nha_api_kit: http-engine, часть 3

Проверочная задача главы. Движок из части 2 научился держать соединения и отбиваться от злых клиентов. Теперь он
учится быть сервером для браузера и для файлов: слой CORS, cookie, условные запросы и докачка по `Range`,
потоковый читатель тела и маленький сервис хранения файлов в памяти, на котором всё это работает вместе.

## Что уже есть

Движок части 2 целиком (`parse.rs`, `router.rs`, `json.rs`, `server.rs`, `middleware.rs`), его приёмочные тесты
(`tests/part2.rs`: сервер не должен сломаться) и заготовки пяти новых модулей. Готовы: `SetCookie::new`,
`delete`, `parse_cookies` и `cookie`, `etag`, `Framing`, вспомогательные методы читателя тела, `FileStore`.
Остальное падает через `todo!()`. Приёмочные тесты нового лежат в `tests/acceptance.rs`.

## Что сделать

1. `cors.rs`: `cors(policy) -> Middleware`. Предзапрос (OPTIONS с `Origin` и `Access-Control-Request-Method`) слой
   обрабатывает сам и не зовёт обработчик: 204 с `Allow-Origin`, `Allow-Methods`, `Allow-Headers`, `Max-Age`
   (и `Allow-Credentials`), `Vary` по трём заголовкам запроса; если origin, метод или любой запрошенный заголовок не
   разрешены, это 403 без разрешений. Остальные запросы проходят к обработчику, а к ответу любого статуса добавляются
   разрешения: точный origin из списка (при шаблоне `*` и включённых `credentials` тоже конкретный origin, а не звёздочка),
   `Allow-Credentials`, `Expose-Headers`; `Vary: Origin` добавляется всегда, кроме постоянной звёздочки без `credentials`,
   и не затирает чужой `Vary`.
2. `cookie.rs`: `SetCookie::to_header`. Порядок атрибутов: `Max-Age`, `Path`, `Domain`, `HttpOnly`, `Secure`, `SameSite`.
   Ошибки: имя не токен, значение с запрещёнными символами, `;` или управляющие символы в атрибутах,
   `SameSite=None` без `Secure`, нарушенные обещания `__Host-` и `__Secure-`.
3. `conditional.rs`: `etag_matches` (список, `W/`, `*`, запятая внутри кавычек), `parse_range` (`Full`, `Partial`,
   `Unsatisfiable`) и `serve_bytes(request, content_type, data)`. Сначала `If-None-Match` (304 с `ETag` и
   `Cache-Control: no-cache`, без тела), потом `Range` (206 с `Content-Range`, 416 с `bytes */размер`), иначе 200.
   Во всех ответах `ETag`, `Accept-Ranges: bytes` и `Content-Type`, кроме 304 и 416. `Range` действует только для GET и HEAD.
4. `body.rs`: `BodyReader::next_chunk`: тело кусками не больше 4096 байт для `Content-Length` и chunked (трейлеры
   читаются и выбрасываются), лимит по заявленной длине вперёд и по сумме для chunked, остаток за телом не трогается.
5. `files.rs`: `files_router(store, max_upload)`: `GET /files/:name` (через `serve_bytes`, 404 JSON-ошибкой),
   `PUT /files/:name` (тело больше `max_upload` это 413; `Content-Type` запоминается; 201 при создании, 204 при замене, в
   ответе `ETag`), `DELETE /files/:name` (204 или 404). `HEAD` обслуживает роутер через `GET`.

## Критерии приёмки

`cargo test`. Проверяется:

- разбор и выдача cookie, удаление, отказы на опасные значения;
- CORS: предзапрос (успех и все отказы), разрешения на ошибочных ответах, шаблон с `credentials`, `Vary`;
- условные запросы: список и слабые теги в `If-None-Match`, 304, 206, 416, испорченный `Range` как 200, порядок проверок;
- читатель тела: куски, лимиты, chunked при любой нарезке, остаток, обрыв, `drain`;
- файловый сервис по сети: загрузка, замена, условный GET, докачка, HEAD, 413 без поломки соединения, удаление;
- CORS-слой поверх файлового сервиса и мягкая остановка прежнего движка.

## Структура

```
nha_api_kit/
  Cargo.toml
  src/lib.rs, parse.rs, router.rs, json.rs, server.rs, middleware.rs   готово
  src/cors.rs, cookie.rs, conditional.rs, body.rs, files.rs            в каждом файле свои функции с todo!()
  tests/part2.rs, tests/acceptance.rs
```

## Подсказки по теории

README главы: cookie и атрибуты, CORS и предзапрос, условные запросы, Range, потоковое тело. Из задач:
nha1 и nha2 для `cookie.rs`, nha3 до nha5 для `cors.rs`, nha6 и nha7 для `conditional.rs`, nha8 для `body.rs`. Читатель
тела пока не встроен в сервер: движок читает тело сам и отдаёт обработчику целым, а `BodyReader` пригодится в потоковых
обработчиках следующих глав и в аватарах социальной сети.
