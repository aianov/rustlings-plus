# Веха 19. Порты

Скоро соцсеть переедет в настоящие базы: Postgres для профилей и постов, Scylla для переписки. Сейчас ядро не готово к этому совсем: `App` держит `Mutex<Network>` и ходит в неё напрямую, команды читают сеть замыканием под замком, а синхронная шина команд ждёт ответа через `block_on`. С базой так не выйдет: каждый её ответ это `.await`, и замыкание под замком его не дождётся.

Пора провести границу. Хранилища прячутся за асинхронными портами `UserRepo`, `PostRepo` и `ChatRepo`, а `App` держит только `Arc<dyn …>` и не знает, что под ними: сеть в памяти сейчас, база потом. Синхронную шину команд сменяет асинхронная таблица методов `service.method`, куда кладутся обычные `async fn`. А чтобы проверить, что ядро и правда не ждёт портов по очереди, в test-support появляется медленная обёртка над ними.

## Что сделать

1. **contracts.** `Command::method(&self) -> &'static str`: метод `service.method` у каждой команды (`register` это `users.register`, `post` это `posts.create`, `follow` `users.follow`, `like` `posts.like`, `feed` `posts.feed`, `inbox` `notify.inbox`, `mute` `users.mute`, `delete` `posts.delete`, `page` `posts.page`, `posts` `posts.author`, `comment` `comments.create`, `reply` `comments.reply`, `thread` `comments.thread`, `trends` `posts.trends`, `stats` `admin.stats`, `import` `admin.import`, `set` `admin.set`, `card` `users.card`, сообщение `chat.send`, история `chat.history`). `COMMAND_METHODS: [&str; 20]` перечисляет их все. Методы, которые совпадают с JSON-методами m13, называются так же.

2. **social-core, `ports/`.** Папка из трёх файлов: `mod.rs` (только `mod` и `pub use`), `store.rs` (прежние синхронные `UserStore` и `PostStore`, на них стоит `Network`) и `repo.rs`:
   - `pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;` Свой, без `futures`;
   - `UserRepo: Send + Sync`: `register(name: String)`, `user(id) -> Option<User>`, `find(name: String) -> Option<User>`, `follow`, `mute` (`Result<bool, DomainError>`), `is_following`, `follow_counts(id) -> FollowCounts { followers, following }`, `count()`;
   - `PostRepo: Send + Sync`: `publish(author, text: String, visibility) -> Result<Published { post, audience }, _>`, `like(user, post) -> Result<Liked { changed, author }, _>`, `delete`, `post(id) -> Option<PostItem>`, `feed(viewer, skip, take) -> Vec<PostItem>`, `by_author(viewer, author, after, size) -> Result<PostPage { posts, next }, _>`, `author_stats(author) -> AuthorStats { posts, likes }`, `comment`, `reply`, `replies(viewer, post, parent: Option<CommentId>) -> Result<Vec<Comment>, _>` (прямые ответы одного уровня, `None` это комментарии к самому посту, в порядке id), `trend_snapshot() -> (Trends, Timestamp)`, `count()`;
   - `ChatRepo: Send + Sync`: `send(from, to, text: String) -> Result<Message, _>`, `history(user, peer, limit) -> Result<Vec<Message>, _>`;
   - все методы берут `&self` и отдают `BoxFuture<'_, …>`, строки приходят по владению (`String`), чтобы future не держал чужих ссылок;
   - `PostItem { id, author, author_name: Arc<str>, text: Arc<str>, kind, visibility, time, likes }` (`Clone`, `Eq`) это готовая к показу копия поста: `PostItem::of(&Post, Option<&User>)`, `cursor()`, `Display` как у `Post` (`[7] превью (лайков: 2)`). Чтобы копия ничего не стоила, `User` хранит ник, а `Post` текст в `Arc<str>` (`User::shared_name()`, `Post::shared_text()`, прежние `name()` и `text()` отдают `&str`);
   - `Repos { users: Arc<dyn UserRepo>, posts: Arc<dyn PostRepo>, chats: Arc<dyn ChatRepo> }` (`Clone`).
   - Экспорт из корня: все эти типы и `Trends`.

3. **social-core, `storage/mem/`.** Папка `storage/` теперь только с `mem/`: `users.rs` и `posts.rs` (прежние `MemUserStore`, `MemPostStore`), `repo.rs` и `app.rs`. `MemRepo<U, P>` держит `Mutex<Network<U, P>>` и реализует все три порта: каждый метод берёт замок на одну синхронную секцию внутри `async move`. `MemRepo::repos(network) -> Repos` раздаёт один `Arc<MemRepo>` всем трём портам. `Network` получает `replies(viewer, post, parent)`. В `app.rs` живут конструкторы фасада в памяти: `App::new(network)`, `App::with_notifiers(network, notifiers)` и `App::start(network, notifiers, config)` для любого `Network<U, P>`, они собирают `MemRepo::repos` и зовут общий `App::with_repos`.

4. **social-core, `app/`: фасад над портами.** `App` больше не обобщённый: `App::with_repos(repos: Repos, notifiers, config) -> social_core::Result<App>`. В `app/` нет ни `Network`, ни `storage::`.
   - Операции идут через порты, события создаёт сам фасад после удачной операции: подписка (если добавилась), лайк (если засчитан, автор из `Liked`), новый пост, сообщение. Событие уходит синхронным уведомителям (`Notifiers::deliver`) и в broadcast. Синхронной шины сети фасад больше не касается, `App::subscribe`, `unsubscribe` и `read` уходят;
   - `app/queries.rs`: `user`, `find_user`, `user_count`, `post_count`, `is_following`, `follow_counts`, `author_stats`, `post`, `feed(viewer, skip, take)`, `author_page`, `history`, все `async`;
   - `profile(id)` теперь `async`: попадание в кэш портов не трогает;
   - `thread(viewer, post) -> Result<Vec<(usize, Comment)>, AppError>` грузит ветку рекурсивной async-функцией: ответы на пост, для каждого его ответы с глубиной на один больше и так далее, в глубину. Рекурсивный вызов заворачивается в `Box::pin`, а функция отдаёт `BoxFuture<'a, …>`;
   - карточка профиля собирает части из портов (`profile`, `follow_counts` и `author_stats` вместе через `join!`, `author_page`, `is_following`) всё тем же `try_join!` под дедлайном;
   - импорт, тренды, рассылка и настройки работают как в m18, только через порты.

5. **social-core, `commands/`: таблица вместо шины.** `CommandBus`, `Context` и `network_commands` уходят вместе с `bus.rs` и `context.rs`.
   - `commands/table.rs`: `pub type Handler = Box<dyn Fn(App, Command) -> BoxFuture<'static, Result<String, CommandError>> + Send + Sync>;` и `CommandTable`: `new()`, `route(method, handler) -> bool` (принимает обычную `async fn(App, Command) -> Result<String, CommandError>` и сам заворачивает её в `Box::pin`; `false`, если метод уже был, и обработчик заменяется), `methods()` (по алфавиту), `call(&self, &App, Command).await` по `command.method()`; нет обработчика, значит `CommandError::NoRoute(method)` (500, `у метода admin.stats нет обработчика`). `CommandError::NoExtension` уходит вместе с контекстом.
   - `CommandTable::social()` регистрирует все 20 методов, `social_table() -> &'static CommandTable` строит её один раз (`OnceLock`).
   - Обработчики это обычные `async fn` по сервисам: `users.rs`, `posts.rs`, `comments.rs`, `chat.rs`, `notify.rs`, `admin.rs`. Каждый достаёт свой вариант команды через let-else. Тексты ответов прежние.
   - `execute_line(line, &App)` и `execute(&Command, &App)` идут через `social_table()`.
   - Отрисовка (`render.rs`) работает с `PostItem`: ник автора уже в карточке поста.

6. **tools/repl.** Терминал держит рантайм, `App`, `&'static CommandTable` и свою `Session`. Строка с `/` разбирается как команда сессии, остальное как `Command` и уходит в таблицу через `runtime.block_on`. Ответы и коды прежние. `session()` по-прежнему `Option<&Session>`.

7. **test-support.** `SlowRepos` (за `test-util`, крейт получает `tokio` как optional): `SlowRepos::wrap(repos, delay)` и `SlowRepos::over(network, delay)` оборачивают каждый порт так, что каждый вызов сначала ждёт `tokio::time::sleep(delay)`, `repos()` отдаёт обёрнутые `Repos`, `calls()` считает вызовы портов, `delay()`. С замороженным временем такие тесты идут мгновенно и точно.

8. **Старые тесты.** Веха кладёт новые версии тестов ядра, которые знали `App<U, P>`, `read`, `subscribe` или шину: `social-core/tests/journey/` m06, m07, m10, m11 (тесты шины и контекста уходят, подписчик App слушает `events()`), m12, m13, m14, m15 (уведомитель ходит в ядро своей задачей, кэш профилей проверяется по счётчику вызовов портов), m16 (бюджет аллокаций: не больше 10 на команду и не растёт с размером страницы, каждый вызов порта стоит коробку под future, пост ни одной), m17, m18.

## Дерево после вехи

```
backend/
  src/crates/
    contracts/
      src/command/mod.rs            + Command::method, COMMAND_METHODS
      tests/journey/m19_async_ports.rs      тесты курса (и строка mod в tests/journey/main.rs)
    social-core/
      src/
        lib.rs                      + порты, MemRepo, CommandTable, social_table, Trends;
                                      без CommandBus, Context, network_commands
        domain/user.rs              ник в Arc<str>
        domain/post.rs              текст в Arc<str>
        ports/
          mod.rs
          store.rs                  UserStore, PostStore
          repo.rs                   BoxFuture, UserRepo, PostRepo, ChatRepo, Repos, PostItem...
        storage/
          mod.rs
          mem/
            mod.rs
            users.rs                MemUserStore
            posts.rs                MemPostStore
            repo.rs                 MemRepo: три порта над Mutex<Network>
            app.rs                  App::new, with_notifiers, start над памятью
        network/comments.rs         + replies
        comments/mod.rs             + children
        app/
          mod.rs                    App над Repos, события из фасада
          queries.rs                запросы и рекурсивная ветка
          card.rs                   части из портов
          import.rs, trends.rs, heavy.rs, settings.rs   без Network
        commands/
          mod.rs                    social_table, execute, execute_line
          table.rs                  CommandTable, Handler
          users.rs posts.rs comments.rs chat.rs notify.rs admin.rs   обработчики
          render.rs                 PostItem вместо Post
          bus.rs, context.rs        удалены
        api/mod.rs                  через запросы App
      tests/journey/
        m19_async_ports.rs          тесты курса
        m06 m07 m10 m11 m12 m13 m14 m15 m16 m17 m18   новые версии
    test-support/
      Cargo.toml                    + tokio (optional, test-util)
      src/slow/
        mod.rs                      SlowRepos
        users.rs posts.rs chats.rs  медленные порты
  tools/repl/src/terminal/mod.rs    таблица и своя сессия
```

Остальное дерево не меняется.

## Приёмка

- `src/crates/social-core/tests/journey/m19_async_ports.rs`: App собирается из портов и пишет в них, фейковые хранилища под портами, future всех методов App, `execute_line` и таблицы `Send`, части карточки идут одновременно на медленных портах, дедлайн карточки на очень медленных, ветка комментариев в глубину и командой `thread`, один вызов порта на комментарий, таблица знает все методы, обычная `async fn` как обработчик и `NoRoute`;
- `src/crates/contracts/tests/journey/m19_async_ports.rs`: метод у каждой команды, `COMMAND_METHODS` без повторов и в форме `service.method`;
- новые версии старых тестов и остальные тесты прошлых вех проходят;
- `cargo make check` зелёный.

Архитектура: порты в `ports/repo.rs` (`Send + Sync`, свой `BoxFuture`, `Arc<dyn …>` в `Repos`); `App` без `Network` и `storage::`; хранилища в `storage/mem/`, а `storage/users.rs`, `storage/posts.rs`, `commands/bus.rs` и `commands/context.rs` удалены; рекурсия через `Box::pin`; таблица на `BoxFuture`; в ядре нет `block_on`, `async-trait` и `futures`; и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
printf 'register alice\nregister bob\npost alice Порты!\ncomment bob 1 Наконец-то\nreply alice 1 Ага\nthread bob 1\ncard bob alice\n' | cargo run -q -p repl
```

Ветку комментариев ядро собирает рекурсивно через порт, а карточку из четырёх частей одновременно.
