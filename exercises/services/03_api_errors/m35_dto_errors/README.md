# Веха 35. Контракт входа и выхода

Соцсеть отвечает на ошибки как придётся. Кривой ник при регистрации это 400 с текстом serde, в котором видно присланное значение; пустой пост это `rejected`, а пустое сообщение тоже `rejected`, хотя одно правило переписки, а другое ошибка поля; клиент узнаёт об ошибках по одной за запрос; шлюз отвечает на 404 роутера телом `{"error": "текст"}`, а на ошибку сервиса телом `{"error": {"code", "message"}}`; поддержке не за что зацепиться, потому что номера запроса в ответе нет. Эта веха ставит на границе каждого сервиса DTO и свой валидатор, а наружу отдаёт один конверт ошибки со стабильным кодом, полями и номером запроса.

Собирается всё из глав 02_dto_validation и 03_api_errors: упражнения с svdto1 по svdto16, svdto_validator, svdto_grpc, svdto_quiz, с sverr1 по sverr12, sverr_grpc и sverr_quiz. Заодно соцсеть получает новое: подпись и «о себе» в профиле (PATCH в три состояния), почту при регистрации, теги и ссылки в постах.

## Что сделать

1. **src/crates/validation** (новый крейт без зависимостей).
   - `pattern/`: `Pattern::parse(source) -> Result<Pattern, PatternError>` и `is_match(text)` (символы, `.`, классы `[a-z0-9_]` и `[^...]`, `\`, `?`, `*`, `+`, `{n}`, `{n,}`, `{n,m}`; совпадение со всей строкой; перебор с запоминанием провалов, без экспоненты). `PatternError { Unclosed, EmptyClass, BadRange, BadEscape, NothingToRepeat, BadQuantifier }`.
   - `rule/`: `Violation { code: &'static str, message }`, трейт `Rule<T: ?Sized>` с `check(&T) -> Result<(), Violation>` (замыкание `Fn(&T) -> Result<(), Violation>` тоже правило), `RuleExt::and` (первое нарушение побеждает). Правила: `NotBlank` (`blank`), `CharLen { min, max }` в символах (`too_short`, `too_long`), `NoControl` (`control_char`), `Matches { pattern, what }` (`invalid_format`, а сломанный шаблон это `bad_pattern`: правило отказывает, а не пропускает всё), `Range { min, max }` (`out_of_range`), `MaxItems(n)` (`too_many`), `Unique` (`duplicate`), `OneOf(&[..])` (`not_allowed`), `HttpUrl` (`invalid_url`: только http и https, есть хост, без пробелов и `user@`, порт до 65535; длиннее 2048 это `too_long`).
   - `validator/`: `Validator` (`check(field, &value, &rule) -> bool`, `parse::<T>(field, raw) -> Option<T>` для любого `TryFrom<&str>` с ошибкой `Into<Violation>`, `report`, `nested(field, ..)`, `each(field, items, ..)` с путями вида `author.links[2].url`, `is_clean`, `finish() -> Result<(), Invalid>`, `finish_with(Option<T>) -> Result<T, Invalid>`), `FieldError { path, code, message }`, `Invalid { errors }` с `pairs()`, `collapse_spaces`.

2. **contracts** (новая зависимость `validation`).
   - `errors/`: каталог `ErrorCode` (`ALL`, `as_str`, `from_wire`, `http_status`, `for_status`, `is_client_fault`, `is_temporary`, `public_message`, `Display`, сравнение со строкой). Строки прежние плюс две новые: `validation` (422, поля в `details`) и `timeout` (504). `ErrorBody { code, message, details, request_id }` переехал сюда; `details` и `request_id` не пишутся, когда пусты, поэтому старые клиенты видят прежний JSON. `Detail { field, code, message }`, `ErrorBody::{new, public, invalid, with_request_id, known, http_status, fields}`. `parse_body::<T>(bytes)` и `json_error`: синтаксис это `syntax` 400, обрыв это `truncated` 400, не та форма это `validation` 422 с полем (`missing`, `unknown_field` или `wrong_type`) и без текста serde, в котором бывают присланные значения.
   - `email/`: `Email` с `TryFrom<&str>` (trim, нижний регистр, `blank`, `too_long`, `invalid_format`) и serde через `try_from`. `NameError` переводится в `Violation` (`blank`, `too_long`, `invalid_char`). `MIN_NEW_NAME_LEN = 3`: правило для новых ников, искать старые короткие по-прежнему можно.
   - `patch/`: `Field<T> { Missing, Null, Value(T) }` со своим `Deserialize` и `ProfilePatch { display_name, bio }` с `deny_unknown_fields`.
   - `UserView` получает `display_name` и `bio`, `PostView` получает `tags` и `links: Vec<LinkView { url, title }>` (все пропускаются, когда пусты), `UserView::named`. `Call::CreatePost { user, text, tags, links }`. `Error::kind() -> ErrorCode`.

3. **proto**. `users.proto`: в `Profile` поля `display_name` и `bio`, новый `rpc UpdateProfile(UpdateProfileRequest)` с `optional string display_name`, `optional string bio` и `repeated string clear_fields`. `posts.proto`: `message Link { url, title }`, в `Post` и `CreatePostRequest` поля `tags` и `links`. `status/`: `grpc_code(ErrorCode)`, `to_status(&ErrorBody)` (код в `x-error-code`, поля в бинарных метаданных `x-error-details-bin`, внутренний текст не уходит), `invalid(&Invalid)`, `from_status(&Status, service) -> ErrorBody` (код соседа сверяется с gRPC-кодом, незнакомый или противоречивый становится `internal`, `Unavailable` и `Cancelled` это `unavailable`, `DeadlineExceeded` это `timeout`). `domain`, `domain_code` и `explain` остаются поверх новых функций.

4. **user-service**.
   - `common/profile.rs`: `DisplayName` (пробелы схлопнуты, 1..=40 символов, без управляющих), `Bio` (1..=160, переводы строк можно), `Patch<T> { Keep, Clear, Set(T) }`, `ProfileChange { display_name, bio }` с `apply(&User) -> User`. `User` получает `display_name` и `bio`, `User::new`.
   - `dto/`: `new_profile(&CreateProfileRequest) -> Result<Username, Invalid>` (ник плюс `MIN_NEW_NAME_LEN`), `profile_change(&UpdateProfileRequest) -> Result<ProfileChange, Invalid>` (неизвестное имя в `clear_fields[i]` это `not_allowed`, поле одновременно задано и очищено это `set_and_clear`, пустая строка не очищает, а нарушает `blank`), `profile` с новыми полями, `CLEARABLE`.
   - `repository`: `UserRepository::update(id, change)` применяет `change.apply` под замком и пишет в журнал `Updated { id, display_name, bio }`; при разборе журнала номер строки настоящий.
   - `service`: `register(Username)`, `update(who, ProfileChange)`, `UserError::code() -> ErrorCode` (вариант `BadName` ушёл в DTO).
   - `controller`: `update_profile`, ошибки DTO уходят через `invalid(&errors)`, ошибки сервиса через `to_status`, `internal` пишется в лог цепочкой `report`.

5. **post-service**.
   - `common/draft.rs`: `PostText`, `CommentText`, `Tag` (снимается `#`, нижний регистр, `TAG_PATTERN` из validation, до 30 символов), `Link { url, title }`, `Draft { text, tags, links }`, `MAX_TAGS = 5`, `MAX_LINKS = 3`, `MAX_LINK_TITLE = 80`. `Post` получает `tags` и `links`.
   - `dto/`: `draft(&CreatePostRequest) -> Result<Draft, Invalid>` (текст, теги с путями `tags[i]`, повтор после нормализации это `tags: duplicate`, ссылки с путями `links[i].url` и `links[i].title`), `comment_text`, ответ с тегами и ссылками.
   - `repository/entry.rs`: `PostEntry` (у `Posted` поля `tags` и `links: Vec<LinkEntry>` с `serde(default)`, старый журнал читается), `insert(author, name, Draft, at)`, `comment(.., CommentText)`.
   - `service`: `create(&dyn UserDirectory, author, Draft)`, `comment(.., CommentText)`. `PostError::code() -> ErrorCode` и `body()`: автора нет у соседа это `validation` с полем `author` (gRPC `InvalidArgument`), сосед лежит это `unavailable`.

6. **chat-service**: `MessageText` (`MAX_MESSAGE_LEN` символов, пустое это `blank`) и `dto::text(&SendRequest)`; `ChatService::send(.., MessageText)`; «себе» остаётся правилом `rejected` в service.

7. **auth-service**: `Account.email: Option<String>` (старый журнал читается), `Registration { username, email }`, `dto::RegistrationBody` (`deny_unknown_fields`) и `LoginBody`, `dto::registration` и `dto::login`; `http::json(&request, rid)` (415 на чужой Content-Type, дальше `parse_body`), `failure(ErrorBody)` с заголовком `x-request-id`, `fail(ErrorCode, ..)`; `AuthService::register(&dyn ProfileDirectory, &Registration)`; `ProfileError::Refused(ErrorBody)`; `AuthError::code() -> ErrorCode` и `body()`.

8. **api-gateway** (новая зависимость `serde`).
   - `controller/errors.rs`: слой `request_ids()` сразу за CORS. Он даёт каждому HTTP-запросу свежий `x-request-id` (присланный клиентом не берётся), ставит его в ответ и приводит любое тело ошибки, даже 404 роутера и 401 проверки токена, к конверту `{"error": {code, message, details, request_id}}`.
   - `fail(ErrorCode, ..)`, `failure(ErrorBody)`, статус из каталога, `json::<T>(&request)` для тел; `stamp` берёт номер запроса из заголовка.
   - `PATCH /users/me` с `ProfilePatch` (`dto::profile_update`): не прислано значит не трогать, `null` значит очистить.
   - WS: `posts.create` с `tags` и `links`, ошибки маршрутов получают `request_id`, паника обработчика это `internal` с публичным текстом и записью в лог.
   - `client::failure` строится на `from_status`: доменный код проходит со своим текстом и полями, внутренний текст соседа не пересылается, а пишется в лог.

9. **tools/social-cli** приходит новой версией: ошибка печатается как `ошибка (код[, HTTP статус]): текст` и строками `поле: код (сообщение)`, у серверных ошибок номер запроса для поддержки; `register <ник> [почта]`, `post` превращает слова с `#` в теги, а `http(s)://` в ссылки; `set name|bio <текст>` и `clear name|bio`; профиль и пост показывают новые поля.

## Дерево после вехи

```
backend/src/
  crates/
    validation/src/pattern/ rule/ validator/   свой валидатор без зависимостей
    contracts/src/errors/ email/ patch/        каталог кодов, конверт, Email, PATCH
    proto/proto/social/v1/                     UpdateProfile, Link, tags и links
  microservices/
    user-service/src/common/profile.rs         DisplayName, Bio, Patch, ProfileChange
    post-service/src/common/draft.rs           PostText, Tag, Link, Draft
    post-service/src/repository/entry.rs       PostEntry, LinkEntry
    api-gateway/src/controller/errors.rs       x-request-id и единый конверт
tools/social-cli/                              ошибки по полям (новая версия)
```

## Приёмка

- `validation/tests/journey/m35_dto_errors.rs`: коды правил, длина в символах, ссылки, комбинатор и замыкания, мини-шаблон и его ошибки разбора, сломанный шаблон отказывает, пути `author.links[2].url`, все ошибки разом, `finish_with`.
- `contracts`: строки каталога как публичный контракт, статусы, конверт без новых полей совпадает со старым, незнакомый код не ломает разбор, 400 на синтаксис и обрыв и 422 на форму без эха значений, `Email`, `Field` в трёх состояниях, новые поля видов.
- `proto`: каждый код с gRPC-кодом, поля туда и обратно через бинарные метаданные, внутренний и противоречивый текст не проходит, недоступность и таймаут называют сервис.
- `user-service`: нормализация и проверка ника, PATCH в трёх состояниях и все его ошибки разом, `UpdateProfile` целиком или никак, профиль переживает перезапуск, два одновременных PATCH не теряют друг друга.
- `post-service`: черновик с тегами и ссылками и все его ошибки с путями, теги и ссылки в журнале, ответ соседа по смыслу (автора нет: `InvalidArgument`, сосед лежит: `Unavailable`).
- `chat-service`: текст сообщения на границе; `auth-service`: регистрация называет ник и почту разом, 400, 415 и 422 с `request_id` в теле и заголовке, лишнее поле `admin` не проходит, почта в журнале.
- `api-gateway`: PATCH /users/me в трёх состояниях и без частичного применения, единый конверт у 401 и 404, теги и ссылки через WebSocket и GET /posts/:id, паника обработчика, клиент курса показывает поля и коды.
- новые версии тестов прошлых вех там, где поменялся контракт: m13, m25 и m31 contracts, m30 proto, m25 шлюза (и его `fakes.rs`, `m32_mtls.rs`), m30 и m33 auth-service, m31 и m33 chat-service, m30, m31 и m33 post-service, m33 user-service;
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву, `validation` в workspace и без зависимостей, каталог `ErrorCode` и конверт в `contracts/src/errors`, бинарные метаданные и сверка кодов в proto, `Validator::new()` в dto каждого сервиса, `invalid(&errors)` в контроллерах, нигде нет `fn status_of`, в service нет `Deserialize`, PATCH под замком репозитория, `request_ids()` и `PATCH /users/me` в шлюзе, теги через `Matches`, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

В клиенте попробуй `register ab nope`: ответ назовёт оба поля сразу (`username: too_short`, `email: invalid_format`). Потом `register alice alice@example.com`, `login alice`, `post читаю #rust про tokio https://tokio.rs`, `set name Алиса`, `me`, `clear name`. Во втором окне `curl -si --cacert certs/ca.pem https://127.0.0.1:7880/nowhere`: даже 404 роутера приходит в конверте `{"error": {"code": "not_found", ...}}`, и `request_id` в теле совпадает с заголовком `x-request-id`.
