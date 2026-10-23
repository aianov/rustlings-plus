# gRPC-ошибки post-service

Мобильное приложение ходит в post-service по gRPC, и любая ошибка выглядит там одинаково: статус `Unknown` с
текстом вроде `NotFound(404)`. Приложение не может отличить «такого поста нет» от «сервис упал»,
не знает, стоит ли повторять запрос, а в поддержку пользователи присылают скриншот без номера запроса.
Ещё хуже, что в текст внутренней ошибки попадает всё, что написал драйвер базы. Твоя задача: научить
сервис отвечать структурированными ошибками, а клиентскую сторону их разбирать.

## Что уже есть

- Воркспейс с пакетом `src/microservices/post-service`, `build.rs`, генерирующий код из `proto/post.proto`
  (`post_service::pb`).
- `src/domain.rs`: `PostService` и доменные ошибки `PostError`. Его менять не нужно.
- `src/controller.rs`: контроллер, который превращает любую ошибку в `Status::unknown` с Debug-текстом.
- `src/errors.rs`: заготовка с сигнатурами и константами ключей метаданных.
- Приёмочные тесты `tests/acceptance.rs`. Править их нельзя.

## Что сделать

1. В `errors::request_id` возьми номер запроса из метаданных `x-request-id`; если его нет (или он пустой),
   выдай свой, и у разных запросов он разный.
2. В `errors::status_from` собери `Status` из `PostError`:

   | ошибка | код gRPC | `x-error-code` | сообщение |
   |---|---|---|---|
   | `NotFound(id)` | `NotFound` | `post.not_found` | называет пост |
   | `AuthorNotFound(id)` | `InvalidArgument` | `post.author_not_found` | называет автора |
   | `TextTooLong` | `InvalidArgument` | `validation.failed` | «текст слишком длинный», а в `x-error-details` значение `text:too_long` |
   | `Unavailable` | `Unavailable` | `service.unavailable` | общее, и `retry-after: 1` |
   | `Internal(_)` | `Internal` | `internal` | общее, текст причины не показывается никак |

   Во всех ответах есть `x-request-id`.
3. В `controller.rs` используй обе функции: запрос получает номер, ошибка превращается в `Status`.
4. В `errors::client_error` разбери `Status` на стороне клиента в `ClientError`: код берётся из метаданных,
   а если их нет (статус сделал прокси), по gRPC-коду (`Unavailable` это `service.unavailable`,
   `DeadlineExceeded` это `service.timeout`, `NotFound` это `not_found`, `InvalidArgument` это
   `validation.failed`, остальное `internal`). Повторять можно `Unavailable`, `DeadlineExceeded` и
   `ResourceExhausted`, решение принимай по gRPC-коду, а не по тексту. `x-error-details` разбирается из
   пар `поле:код` через запятую, `retry-after` из секунд.

## Критерии приёмки

`cargo test --workspace` проходит и `cargo clippy --workspace --all-targets -- -D warnings` чист. Тесты вызывают
контроллер напрямую и один раз ходят настоящим клиентом по TCP на `127.0.0.1:0`, чтобы убедиться, что метаданные
ошибки доезжают по сети. Первая сборка тянет tonic и prost, это может занять несколько минут.

## Подсказки

README главы: раздел «gRPC: код в метаданных». Ключи метаданных в gRPC в нижнем регистре и из ASCII, поэтому значение собирается
через `MetadataValue::try_from`, а не `unwrap` на пользовательской строке.
