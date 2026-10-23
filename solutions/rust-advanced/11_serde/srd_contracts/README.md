# Проект: контракты соцсети

Соцсеть переходит с «каждый сервис придумывает свой формат» на общий словарь. Клиент мобильного приложения,
гейтвей и сервисы должны говорить одним языком: один конверт запроса, один ответ, push-события с тегом и
идентификаторы, которые не потеряются на JavaScript. Крейт `contracts` это и есть такой словарь: только типы
и разбор, никакой сети и никаких зависимостей кроме `serde` и `serde_json`.

## Что нужно сделать

В `crates/contracts` объявлены типы и сигнатуры, тела пустые.

1. `UserId` и `PostId` на проводе строки `u_42` и `p_7`; в Rust это `UserId(u64)` и `PostId(u64)`. Чужой префикс,
   пустая часть, не число и просто число дают ошибку `invalid user id "<текст>": expected u_<number>`
   (для постов `invalid post id ...: expected p_<number>`).
2. `Request { id, service, method, data }`: `data` произвольный JSON (`Value`). `parse_request(raw)` разбирает
   конверт и отличает ошибки: `Truncated` (текст оборван), `Syntax { line, column }` (не JSON),
   `BadEnvelope { reason }` (JSON, но нет нужного поля или не тот тип).
3. `decode_call(&Request)` по имени метода разбирает `data` в `Call`:
   `users.register` (`{"name"}`), `posts.create` (`{"text"}`), `users.follow` (`{"user": "u_7"}`),
   `chat.send` (`{"chat": 3, "text": ".."}`). Неизвестный метод это `UnknownMethod(имя)`, данные не по форме
   это `BadData { method, reason }`. `data` не копируется. `parse_call(raw)` делает оба шага и возвращает
   `(id, Call)`.
4. `Response<T>`: плоский объект `{"id": 1, "ok": ..}` или `{"id": 1, "error": {"code": "..", "message": ".."}}`.
5. `Event`: push-события с тегом `type` в `snake_case`: `post_created { post, author }`,
   `followed { follower, followee }`, `chat_message { chat, from, text }`.
6. `Envelope<T>` с функциями `encode` и `decode`: общий конверт `{"id", "data"}` для любого `T`.
7. `ContractError`: `Display` без хвоста про строку и столбец (тексты ниже), `std::error::Error`, годится для потоков.
   `to_body()` превращает ошибку в `ErrorBody { code, message }`, где `code` это `truncated`, `syntax`,
   `bad_envelope`, `unknown_method` или `bad_data`.

Тексты ошибок: `message is incomplete`, `invalid JSON at line L column C`, `bad envelope: <reason>`,
`unknown method <m>`, `bad data for <method>: <reason>`. Причина (`reason`) это текст ошибки serde без
хвоста ` at line .. column ..`.

## Как проверить

```
cargo test --workspace
```

Приёмочные тесты лежат в `crates/contracts/tests/acceptance.rs`, их менять нельзя.

## Структура

```
Cargo.toml
crates/contracts/
  Cargo.toml
  src/lib.rs        модули и pub use
  src/ids.rs        UserId, PostId
  src/envelope.rs   Request, Call, Response, Envelope, разбор
  src/event.rs      Event
  src/error.rs      ContractError, ErrorBody
  tests/acceptance.rs
```
