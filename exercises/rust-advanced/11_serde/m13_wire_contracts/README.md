# Веха 13. Соцсеть говорит JSON

Пока с соцсетью разговаривает только человек за терминалом: `post alice Привет`, ответ строкой по-русски. Скоро к ней придут программы: клиент, шлюз, соседние сервисы. Им строка «опубликован пост [3]» ни о чём не говорит. Нужен один строгий язык на всех: конверт запроса с номером, ответ с тем же номером, код ошибки, по которому можно принять решение, и события, которые сервер присылает сам, когда тебя лайкнули. Заодно пора навести порядок в ошибках. Сейчас кривые байты на входе роняют терминал с `Error: Custom { kind: InvalidData, .. }`, и это всё, что узнает человек.

Поэтому протокол поселится в contracts, ядро научится его исполнять, а терминал получит флаг `--json`: строка JSON на вход, строка JSON на выход. Этому же контракту потом будет следовать клиент курса, так что имена, формы и тексты ниже фиксированные.

## Что сделать

1. **Воркспейс.** В `[workspace.dependencies]` корня добавь `serde = { version = "1", features = ["derive"] }` и `serde_json = "1"`. contracts подключает оба через `workspace = true`, и больше зависимостей у него нет. Ядро и терминал serde не подключают: JSON читает и пишет только contracts.

2. **contracts, `ids/`: id на проводе.** `UserId` и `PostId` пишутся строками `"u_42"` и `"p_7"`, потому что JavaScript-клиент не удержит большое число. `Display` даёт тот же текст, `FromStr` разбирает его обратно (`Err = IdError`), serde пишет и читает строку. После префикса только цифры, и число помещается в `u64`. Всё остальное (`"7"`, `"u_"`, `"u_+5"`, `"U_1"`, `"p_1"` вместо пользователя, число `7` вместо строки) отказ с текстом `неверный id "7": нужно u_<число>` (у постов `нужно p_<число>`). `IdError` новый публичный тип с `Display` и `Error`. Конструкторы `UserId(1)` и поле `.0` не меняются.

3. **contracts, `username/`.** `Username` на проводе обычная строка (`"alice"`, без `@`). При чтении она проходит те же проверки, что `Username::try_from`: ник нормализуется, а пустой или длинный ник даёт отказ с текстом `NameError`.

4. **contracts, `envelope/`: конверт и ответ.**
   - `Envelope<T = Value> { pub id: u64, pub service: String, pub method: String, pub data: T }`, где `Value` это `serde_json::Value`. Сервер читает `Envelope` (данные пока деревом), клиент пишет `Envelope<СвойТип>`. `Envelope::new(id, method, data)` берёт `service` из метода: часть до первой точки.
   - `Response<T> { pub id: Option<u64>, pub outcome: Outcome<T> }` и `Outcome<T> { Ok(T), Error(ErrorBody) }`. На проводе это плоский объект: `{"id": 1, "ok": {...}}` или `{"id": 1, "error": {"code": "...", "message": "..."}}`. Если конверт не разобрался и номер неизвестен, `"id": null`.
   - `decode<T: DeserializeOwned>(raw: &str) -> contracts::Result<T>` читает любой тип контракта и раскладывает ошибку serde_json по видам (таблица ниже).
   - `write_line<T: Serialize>(out: impl Write, value: &T) -> io::Result<()>` пишет значение одной строкой JSON и `\n`. Перевод строки внутри текста экранируется, так что одно сообщение всегда одна строка. Сломанная запись это ошибка ввода-вывода, а не паника.

5. **contracts, `api/`: методы.** `Call` это вызов с уже проверенными типами, `METHOD_NAMES` десять имён в порядке таблицы, `Call::method(&self) -> &'static str` имя метода. `Call` реализует `Serialize` так, что пишется ровно объектом `data` своего метода, без имени варианта: клиент собирает запрос как `Envelope::new(id, call.method(), &call)`. `decode_call(&Envelope) -> contracts::Result<Call>` это второй этап разбора: по `method` выбирает тип данных и читает его из `data` по ссылке, не копируя дерево. Сначала проверяется, что метод известен, потом что `service` совпадает с частью метода до точки, потом данные.

   | метод | `Call` | `data` | ответ (`ok`) |
   |---|---|---|---|
   | `users.register` | `Register { username }` | `{"username": "alice"}` | `UserView` |
   | `users.find` | `FindUser { username }` | `{"username": "alice"}` | `UserView` |
   | `users.follow` | `Follow { user, target }` | `{"user": "u_2", "target": "u_1"}` | `Changed` |
   | `users.mute` | `Mute { user, target }` | `{"user": "u_2", "target": "u_1"}` | `Changed` |
   | `posts.create` | `CreatePost { user, text }` | `{"user": "u_1", "text": "Привет"}` | `PostView` созданного поста |
   | `posts.like` | `Like { user, post }` | `{"user": "u_2", "post": "p_1"}` | `Changed` |
   | `posts.delete` | `DeletePost { user, post }` | `{"user": "u_1", "post": "p_1"}` | `Changed` |
   | `posts.feed` | `Feed { user, page, size }` | `{"user": "u_2", "page": 1, "size": 10}` | `FeedPage` |
   | `chat.send` | `SendMessage { user, peer, text }` | `{"user": "u_1", "peer": "u_2", "text": "привет"}` | `MessageView` |
   | `chat.history` | `History { user, peer, limit }` | `{"user": "u_1", "peer": "u_2", "limit": 20}` | `ChatHistory` |

   `user` везде тот, кто действует. Типы полей: ники это `Username`, id это `UserId` и `PostId`, `page`, `size` и `limit` это `usize`, текст `String`. Без `page` берётся 1, без `size` `DEFAULT_FEED_LIMIT`, без `limit` `DEFAULT_HISTORY_LIMIT`. `limit` вне `1..=MAX_HISTORY_LIMIT` это `BadData` с причиной `лимит истории от 1 до 100, а не 0` (текст `ParseError::LimitOutOfRange`).

   Ответы живут в `api/views.rs`, все с `Serialize` и `Deserialize`, чтобы клиент читал их через `Response<T>`:

   | тип | на проводе |
   |---|---|
   | `UserView { id, username }` | `{"id": "u_1", "username": "alice"}` |
   | `PostView { id, author, author_name, text, likes }` | `{"id": "p_1", "author": "u_1", "author_name": "alice", "text": "Привет", "likes": 0}` |
   | `MessageView { from, to, text }` | `{"from": "u_1", "to": "u_2", "text": "привет"}` |
   | `Changed { changed }` | `{"changed": true}` |
   | `FeedPage { posts }` | `{"posts": [PostView, ...]}` |
   | `ChatHistory { messages }` | `{"messages": [MessageView, ...]}` |

   `Reply` это перечисление всех ответов сервера (`User`, `Post`, `Changed`, `Feed`, `Message`, `History`) с `Serialize`, которое пишется как сам ответ, без имени варианта.

6. **contracts, `push/`.** `Push` это событие, которое сервер присылает сам, с тегом `push`: `Followed { to, by }` это `{"push": "followed", "to": "u_1", "by": "u_2"}`, `Liked { to, by, post }` это `{"push": "liked", "to": "u_1", "by": "u_2", "post": "p_1"}`, `Message { to, from }` это `{"push": "message", "to": "u_2", "from": "u_1"}`. `to` тот, кому событие адресовано. Push пишется и читается.

7. **contracts, `error/`.** `contracts::Error`, алиас `contracts::Result<T>`, `ErrorBody { pub code: String, pub message: String }` (на проводе `{"code", "message"}`) и `Error::code(&self) -> &'static str`. `Error` простое значение: `Clone`, `PartialEq`, `Eq`, `Send`, `Sync`, `std::error::Error`. Чужих типов (`serde_json::Error`) внутри нет: позиция и причина хранятся своими полями.

   | вариант | `code` | `Display` |
   |---|---|---|
   | `Truncated` | `truncated` | `конверт оборван` |
   | `Syntax { line, column }` | `syntax` | `не JSON (строка 1, столбец 10)` |
   | `BadEnvelope { reason }` | `bad_envelope` | ``кривой конверт: missing field `data` `` |
   | `UnknownMethod(String)` | `unknown_method` | `неизвестный метод users.fly` |
   | `WrongService { service, method }` | `wrong_service` | `метод users.find не из сервиса posts` |
   | `BadData { method, reason }` | `bad_data` | `плохие данные для users.follow: неверный id "7": нужно u_<число>` |

   Текст оборвался (в том числе пустая строка) это `Truncated`, не JSON это `Syntax`, JSON не той формы это `BadEnvelope`. `reason` это текст ошибки serde без хвоста ` at line .. column ..`.

8. **social-core, `error/`: ошибка ядра.** `social_core::Error` и алиас `social_core::Result<T>`:

   | вариант | `Display` | `source` | `code()` |
   |---|---|---|---|
   | `Contract(contracts::Error)` | `запрос не разобран` | ошибка контракта | `code()` ошибки контракта |
   | `Domain(DomainError)` | `запрос отклонён` | доменная ошибка | `not_found` для `UnknownUser` и `UnknownPost`, `conflict` для `NameTaken`, остальные `rejected` |
   | `UnknownName(String)` | `нет пользователя @carol` | нет | `not_found` |
   | `Start(io::Error)` | `не запустился поток уведомлений` | ошибка ввода-вывода | `internal` |

   `From<contracts::Error>` и `From<DomainError>`, чтобы `?` пересекал границу крейтов сам. `report(error: &(dyn std::error::Error + 'static)) -> String` склеивает `Display` ошибки и всех её причин по `source` через `": "`. `Error::to_body(&self) -> ErrorBody`: код из `code()`, текст из `report`, например `запрос отклонён: ник alice уже занят`. `SharedNetwork::new` и `SharedNetwork::with_notifiers` теперь отдают `social_core::Result` (поток уведомлений не запустился: `Start`). В `src/crates/*/src` нигде нет `Box<dyn Error>`: библиотека отдаёт типизированные ошибки.

9. **social-core, `api/`: исполнение вызовов.**
   - `serve(net: &SharedNetwork<U, P>, call: &Call) -> social_core::Result<Reply>` исполняет вызов через ручку сети. Посты публикуются с видимостью `Public`. `posts.create` и `chat.send` отвечают тем, что сеть сохранила (текст без пробелов по краям). `users.find` с незнакомым ником это `UnknownName`. `posts.feed` это страница `page` (с 1) размером `size` из ленты через `.pages(size)`: за концом ленты, на странице 0 и при `size` 0 пустой список, огромные числа сервер не роняют; лента незнакомого пользователя это `UnknownUser`.
   - `serve_line(net: &SharedNetwork<U, P>, line: &str) -> Response<Reply>` это весь путь одной строки: конверт, вызов, ответ. Ошибка первого этапа даёт ответ с `id: None`, всё остальное отвечает с номером запроса.

10. **social-core, `notify/`: push.** `Notification::to_push(self, to: UserId) -> Push`. `PushQueue` это уведомитель (`impl Notifier`), который копит push-события по порядку: `PushQueue::new()` и `drain(&self) -> Vec<Push>` (забирает накопленное). Свой лайк, как и раньше, никого не уведомляет.

11. **tools/repl: два режима и цепочка причин.**
    - `repl::Error` и алиас `repl::Result<T>`: `Core(social_core::Error)` (`ядро не запустилось`), `Read { line: usize, source: io::Error }` (`не удалось прочитать строку 2`), `Write(io::Error)` (`не удалось записать ответ`). Причина каждого варианта отдаётся через `source`, `From<social_core::Error>` есть. `Terminal::new` и `Terminal::with_notifiers` отдают `repl::Result<Terminal>`.
    - `Mode { Text, Json }` и `repl::run(mode, notifiers: Notifiers, input: impl BufRead, output: impl Write) -> repl::Result<()>` читает строки до конца ввода. Пустые строки пропускаются, но номера строк считаются все, с единицы. `Text` работает как раньше: тело ответа или `ошибка: <тело>`. `Json`: на каждую строку одна строка ответа `serve_line`, сразу за ней push-события, которые эта строка вызвала, по одному на строку. Кривой конверт получает ответ с ошибкой, и сеанс идёт дальше. Ошибка чтения или записи заканчивает сеанс.
    - `main.rs`: флаги `--version`, `--log`, `--json`; любой другой флаг это ошибка `неизвестный флаг --wat`. Работа идёт в функции, которая возвращает `Result<(), Box<dyn Error + Send + Sync>>`. При ошибке main печатает в stderr `ошибка: <цепочка причин через report>` и выходит с кодом неудачи через `ExitCode` (не `process::exit` и не `fn main() -> Result`).

## Дерево после вехи

```
backend/
  Cargo.toml                        + serde и serde_json в [workspace.dependencies]
  src/crates/
    contracts/
      Cargo.toml                    serde и serde_json через workspace = true
      src/
        lib.rs
        ids/mod.rs                  UserId и PostId на проводе, IdError
        username/mod.rs             Username на проводе
        envelope/mod.rs             Envelope, Response, Outcome, decode, write_line
        api/
          mod.rs                    METHOD_NAMES, Call, decode_call
          views.rs                  UserView, PostView, MessageView, Changed, FeedPage, ChatHistory, Reply
        push/mod.rs                 Push
        error/mod.rs                Error, Result, ErrorBody
        chat/  command/  input/
      tests/
        m13_wire_contracts.rs       тесты курса
    social-core/
      src/
        lib.rs
        api/mod.rs                  serve, serve_line
        error/mod.rs                Error, Result, report
        notify/mod.rs               + Notification::to_push, PushQueue
        shared/mod.rs               new и with_notifiers отдают social_core::Result
        chat/  commands/  domain/  events/  feed/  network/  ports/  storage/
      tests/
        m13_wire_contracts.rs       тесты курса
  tools/repl/
    src/
      main.rs                       флаги, Box<dyn Error + Send + Sync>, цепочка причин в stderr
      lib.rs
      error/mod.rs                  Error, Result
      run/mod.rs                    Mode, run: текстовый и JSON-режим
      session/  terminal/
    tests/
      m13_wire_contracts.rs         тесты курса
```

Остальное дерево не меняется.

## Приёмка

- `src/crates/contracts/tests/m13_wire_contracts.rs`: id и ник на проводе и их отказы; конверт и `Envelope::new`; имена методов; форма `data` каждого метода и путь «записал, разобрал, получил тот же Call»; значения по умолчанию и лимит истории; виды ошибок с кодами и текстами; формы ответов, плоский `Response` и чтение `Response<T>` клиентом; push с тегом; одна строка на сообщение;
- `src/crates/social-core/tests/m13_wire_contracts.rs`: `serve` для всех десяти методов, страницы ленты, коды и тексты доменных ошибок, `serve_line` с номером и без, `From` и `source` у ошибки ядра, `report`, push из уведомлений и `PushQueue`;
- `tools/repl/tests/m13_wire_contracts.rs`: JSON-сеанс в процессе и бинарником `repl --json`, push сразу после ответа, текстовый режим как раньше, ошибки чтения и записи с номером строки и причиной, неизвестный флаг, цепочка причин в stderr;
- тесты прошлых вех не меняются и проходят;
- `cargo make check` зелёный.

Архитектура: новые файлы на месте, у contracts из чужих зависимостей только serde и serde_json, ядро не подключает serde, данные в разборе не копируются (`.clone()` в `envelope/` и `api/` нет), `data` конверта это `Value`, у трёх крейтов свой `Result`, ошибки с причиной реализуют `source`, `Box<dyn Error>` только в main терминала, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
printf '%s\n' \
  '{"id": 1, "service": "users", "method": "users.register", "data": {"username": "alice"}}' \
  '{"id": 2, "service": "users", "method": "users.register", "data": {"username": "bob"}}' \
  '{"id": 3, "service": "users", "method": "users.follow", "data": {"user": "u_2", "target": "u_1"}}' \
  '{"id": 4, "service": "posts", "method": "posts.create", "data": {"user": "u_1", "text": "Привет #rust"}}' \
  '{"id": 5, "service": "posts", "method": "posts.feed", "data": {"user": "u_2"}}' \
  '{"id": 6, "service": "users", "method": "users.follow", "data": {"user": "7", "target": "u_1"}}' \
  | cargo run -q -p repl -- --json
```
