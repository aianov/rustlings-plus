# Единый язык ошибок: crate contracts и три сервиса

В соцсети три места, где ошибки видит внешний мир: user-service, post-service и шлюз. Каждое придумало
своё: у одного `404` с текстом, у другого `500` на всё, шлюз пересылает чужие сообщения как есть, а в двух ответах
из десяти торчат имена серверов. Приложение пишет `if`-ы на каждый случай и всё равно ловит ошибки,
которых не ждало. Пора свести всё к одному контракту: каталог кодов, конверт, соответствие
HTTP и gRPC, правила перевода ошибки соседа. Живёт он в `crates/contracts`, а сервисы только подключают его.

## Что уже есть

- Воркспейс из четырёх пакетов: `src/crates/contracts`, `src/microservices/{user-service,post-service,gateway}`.
- `contracts::api`: `User`, `Post` и трейты `UserApi`, `PostApi`, через которые сервисы зовут друг друга (вместо сети).
- `contracts::errors::mod`: `ApiError` (код, необязательное сообщение, `details`, пауза повтора) и сборщики.
- Доменный слой user-service и post-service, роутер шлюза и хелперы (`GrpcStatus`, `GrpcCode`, `HttpReply`).
- Приёмочные тесты в `crates/contracts/tests/acceptance.rs` и `microservices/gateway/tests/acceptance.rs`.
  Править их нельзя.

## Что сделать

В местах с `todo!()` (и там, где `handle` шлюза не защищён):

1. `errors/code.rs`: каталог `Code`. Строки кодов, HTTP-статусы, gRPC-коды, публичные сообщения и деление на
   «вина клиента / наша» (статус ниже 500 это вина клиента). Строки такие: `request.malformed`,
   `validation.failed`, `auth.unauthenticated`, `auth.forbidden`, `user.not_found`, `user.username_taken`,
   `post.not_found`, `post.author_not_found`, `rate_limited`, `service.unavailable`, `service.timeout`, `internal`.
   Статусы: 400, 422, 401, 403, 404, 409, 404, 422, 429, 503, 504, 500. `AuthorNotFound` это ошибка в
   запросе, и в gRPC она `InvalidArgument`. `from_wire` на незнакомой строке возвращает `None`.
2. `errors/http.rs`: `http_reply` собирает ответ с телом `{"error": {code, message, request_id, details?}}`.
   Своё сообщение допустимо только у клиентских ошибок, у серверных всегда публичное. `details` тоже
   только у клиентских и только если не пусты. Заголовки: `content-type`, `x-request-id`, для 401
   `www-authenticate: Bearer`, для 429 и 503 `retry-after`, если пауза задана.
3. `errors/grpc.rs`: `grpc_status` кладёт в метаданные `x-error-code`, `x-request-id`, `x-error-details`
   (пары `поле:код` через запятую, как у клиентских ошибок) и `retry-after`. `from_status` обратно:
   код соседа принимается только если он есть в каталоге и его gRPC-код совпадает с кодом статуса; иначе
   смотри на gRPC-код (`Unavailable`, `DeadlineExceeded`, `ResourceExhausted` имеют смысл, всё остальное
   `Internal`). Чужое сообщение в `ApiError` не копируется.
4. `user-service/controller.rs`, `post-service/controller.rs`: `to_api` переводит доменную ошибку в `ApiError`.
   В post-service ошибка user-service переводится по смыслу: «автора нет» это `AuthorNotFound`, недоступность и
   таймаут сохраняют смысл, всё остальное (включая отказ в правах при вызове сервис-к-сервису) это наша
   ошибка. Текст слишком длинного поста это `ValidationFailed` с полем `text` и кодом `too_long`.
5. `gateway`: `parse_create_post` (битый JSON это `Malformed`, неверные поля это `ValidationFailed` со всеми
   нарушениями в порядке `author_id`, `text`, код `required`, и никаких присланных значений в ответе) и
   `handle` (паника внутри обработки превращается в 500 с номером запроса, текст паники в журнал).

## Критерии приёмки

`cargo test --workspace` проходит и `cargo clippy --workspace --all-targets -- -D warnings` чист. Тесты проверяют
таблицы, формат тела и заголовков, прохождение ошибки через три сервиса, отсутствие внутренних подробностей
в любом ответе и живучесть шлюза после паники.

## Подсказки

Задачи `sverr1`..`sverr12`: почти каждая функция здесь уже была в виде отдельной задачи. Собирай не копированием,
а так, чтобы поведение держалось на одном каталоге `Code`.
