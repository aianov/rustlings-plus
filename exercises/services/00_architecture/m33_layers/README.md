# Веха 33. Слои

Сервисы работают, но внутри каждого лежит чужое ядро: `social-core` писалось ещё для одного процесса, и теперь user-service таскает в себе посты, post-service держит зеркало чужих профилей, а правило «нельзя подписаться на себя» живёт где-то между `App`, `Network` и `MemRepo`. Новый разработчик не найдёт ни одного правила, не прочитав пять тысяч строк. Эта веха раскладывает каждый сервис по слоям и растворяет ядро: у каждого сервиса остаются только его данные и его правила, соседи становятся портами, а хранение прячется за трейтами с атомарными операциями.

Снаружи ничего не меняется: клиент, шлюз, gRPC-контракт и все прежние тесты работают как раньше.

## Что сделать

1. **src/crates/journal** (новый крейт: `serde`, `serde_json`, `tokio`): `Journal<E: Serialize + DeserializeOwned>` это NDJSON на диске, одна запись на строку.
   - `Journal::open(Option<&Path>) -> Result<(Journal<E>, Vec<E>), JournalError>`: создаёт папку, читает записи по порядку, оборванный хвост (строка без `\n`) отрезает `set_len`, испорченная строка посередине это `JournalError::Broken { line, reason }` с номером строки. `None` это журнал в памяти (`Journal::memory()`), он ничего не хранит.
   - `append(&entry)`: строка и `flush` под своим замком, параллельные записи не перемешиваются. `is_durable()`.
   - `JournalError { Io, Broken { line, reason }, Encode }` с `Display` и `source`.

2. **service-runtime**: `report(&dyn Error) -> String` (ошибка и цепочка причин через `: `) для `main.rs` всех сервисов.

3. **Каждый сервис** (user, post, chat, auth) разложен так:

   ```
   src/main.rs         тонкий: адрес, сертификаты, конфиг, run
   src/lib.rs          pub mod слоёв и реэкспорт
   src/bootstrap/      build(...) собирает репозитории, service и controller, run(...) поднимает сервер
   src/controller/     вход: gRPC (и HTTP у auth-service), перевод ошибок в Status, tokio::spawn для записей
   src/dto/            перевод между proto и доменными типами
   src/service/        правила и порты к соседям, ничего не знает про tonic и http_engine
   src/repository/     трейты хранилища в mod.rs, in-memory реализация с журналом в memory.rs
   src/client/         gRPC-клиенты соседей, реализующие порты (у user-service его нет)
   src/common/         конфиг, ошибка запуска, доменные типы
   ```

   Репозитории это трейты с `BoxFuture` (`Pin<Box<dyn Future<Output = T> + Send + 'a>>`) в `repository/mod.rs`, реализация держит состояние под `tokio::sync::Mutex` и пишет журнал внутри той же секции: проверка, запись в журнал и вставка в память это одна операция. Глобальных `static` нет: две сборки `bootstrap::build` не делят данные.

4. **user-service**:
   - `common/`: `User { id: UserId, name: Username }`, `Avatar { content_type, bytes: Arc<Vec<u8>> }`, `Relations { following, followers, muted, audience }`.
   - `repository/`: `UserRepository` (`create_if_absent(name) -> Result<User, RepoError>` с `RepoError::{Conflict, Storage}`, `get`, `find`, `list_after(after, limit)`, `follow`, `mute` (`Result<bool, JournalError>`), `relations`), `AvatarRepository` (`put`, `get`, `remove`), `MemoryUsers` (`new`, `restore(journal, entries)`, журнал `UserEntry { Registered, Followed, Muted }`), `MemoryAvatars`.
   - `service/`: `UserService::new(users, avatars)` с `register` (ник через `Username::try_from`), `profile`, `find`, `list` (`LIST_LIMIT = 500`), `follow`, `mute`, `relations`, `put_avatar`, `avatar`, `delete_avatar`; `UserError` с `code()`: `bad_data`, `conflict`, `not_found`, `rejected` (себя подписать или скрыть), `unsupported`, `too_large`, `internal`. Все правила здесь и только здесь.
   - `controller/`: `UsersController::new(Arc<UserService>)` реализует `Users`.

5. **post-service**:
   - Зеркала профилей больше нет. Имя автора берётся у user-service при создании поста и хранится с постом, подписки читателя спрашиваются перед лентой.
   - `service/directory.rs`: порт `UserDirectory { author(id) -> Result<Option<Author>, DirectoryError>, graph(viewer) -> Result<Option<Graph>, DirectoryError> }`, `DirectoryError::{Unavailable, Failed}`: «нет такого» и «не смогли спросить» это разные ответы.
   - `service/`: `PostService::new(posts)` с `create(&dyn UserDirectory, author, text)` (сначала пусто и длина `MAX_POST_LEN`, потом сосед), `public`, `like`, `delete` (чужой пост: `forbidden`), `feed(users, viewer, page, size)`, `comment`, `trends(limit)` (лайки за `TRENDS_WINDOW`), `watch(users, viewer, recent) -> FeedWatch` (история и живые посты подписок из `broadcast`); `PostError` с `code()`.
   - `repository/`: `PostRepository` (`insert`, `get`, `like_if_absent` атомарно, `delete`, `by_authors(authors, skip, take)`, `comment`, `likes_since`), `MemoryPosts` с журналом `PostEntry`.
   - `client/users.rs`: `GrpcUsers::new(channel, stamp)` реализует `UserDirectory`: controller строит его на каждый вызов со `Stamp::follow(&request)`, поэтому request-id, пользователь и срок по-прежнему едут дальше.
   - `examples/feed_bench.rs`: замер `by_authors` на 50 000 постах, печатает «медиана … мкс, p99 … мкс»; `cargo xtask bench` запускает его (`-p post-service --example feed_bench`).

6. **chat-service**: `common::Message { from, to, text }` и `MAX_MESSAGE_LEN = 500`, `repository::{ChatRepository, MemoryChats, Line}` (журнал `{from, to, text}` прежнего формата), `service::{ChatService, Hub, UserDirectory, ChatError}`: правила «себе», «пустое», «длинное» до похода к соседу, `Hub` раздаёт доменные сообщения и закрывается на остановке, `client::GrpcUsers` спрашивает `GetProfile`.

7. **auth-service**: `common::Account` (serde, тот же формат журнала), `repository::{AccountRepository, TokenRepository, MemoryAccounts, MemoryTokens}` (учётная запись через `create_if_absent`), `service::{AuthService, ProfileDirectory, ProfileError, AuthError}` (`register(&dyn ProfileDirectory, &Username)`, `login`, `logout`, `validate`, `account`), `client::GrpcProfiles`, `controller/` с `http.rs` (слои, `bearer_auth_async` для выхода), `handlers.rs` и `grpc.rs`, `bootstrap::Shared`.

8. **api-gateway**: `bootstrap/` (`run`, `run_with`, `state.rs`), `controller/` (`http.rs`, `users.rs`, `posts.rs`, `avatars.rs`, `ws.rs`, `conn.rs`, `protocol.rs`), `routes/` как было, `client/` (`mod.rs` с `Upstreams`, `auth.rs`, `inbox.rs`), `dto/` (`post_view`, `message_view`, `user_view`, `feed_event`), `common/` (`config.rs`, `error.rs`, `registry.rs`). Публичный API прежний.

9. **Удалить** `src/crates/social-core` и `src/crates/test-support` вместе с их тестами, убрать их из `[workspace.dependencies]` (и `futures`, он больше никому не нужен). У сервисов новые зависимости: `contracts`, `journal`, `serde`; у chat-service и auth-service `serde_json` только в `[dev-dependencies]`.

## Дерево после вехи

```
backend/src/
  crates/
    journal/src/file/ error/            NDJSON-журнал
    service-runtime/src/report/          report()
    social-core/  test-support/          удалены
  microservices/
    user-service/src/   bootstrap controller dto service repository common
    post-service/src/   bootstrap controller dto service repository client common
    post-service/examples/feed_bench.rs
    chat-service/src/   bootstrap controller dto service repository client common
    auth-service/src/   bootstrap controller dto service repository client common
    api-gateway/src/    bootstrap controller routes client dto common
```

## Приёмка

- `journal/tests/journey/m33_layers.rs`: порядок записей, обрыв и испорченная середина, параллельные записи целыми строками, журнал в памяти;
- `user-service`: 32 одновременные регистрации одного ника (журнал на диске) дают одну удачную и 31 `NameTaken`; правила в `UserService`; сбой хранилища за трейтом это `internal`, а дешёвая проверка ника до хранилища; две сборки `bootstrap::build` не делят данные;
- `post-service`: пустой и длинный пост отсекаются без похода к соседу, имя автора из порта, «нет автора» и «сосед лежит» различаются; 32 одновременных лайка одного человека дают один; лента по графу из порта, чужой пост не удалить;
- `chat-service`: правила до соседа, неизвестный собеседник, хаб раздаёт, закрытый хаб не открывает подписок; `auth-service`: доменный отказ соседа проходит со своим кодом, недоступный сосед, вход и отзыв токена;
- новые версии `xtask` m16 (замер это `-p post-service`) и шлюзового m30 (зеркала больше нет: до user-service доходят GetProfile и Relations);
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву, `social-core` и `test-support` отсутствуют, в `src` сервисов только слои (у шлюза свой набор), в controller нет `repository::`, в service нет `tonic::` и `http_engine::`, в repository нет `static`, `LazyLock` и `OnceLock`, атомарные `create_if_absent` и `like_if_absent`, порты `UserDirectory` и `ProfileDirectory` в service и их реализации в client, журнал из крейта `journal`, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
cargo xtask bench
```

Снаружи всё как раньше: `register`, `login`, `post`, `feed --live`, `msg`. Загляни в журналы (`USERS_JOURNAL` и остальные в `.env`): каждая строка теперь пишется сразу и видна на диске до ответа клиенту.
