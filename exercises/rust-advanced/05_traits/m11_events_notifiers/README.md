# Веха 11. События и уведомления

Сейчас ядро знает каждого, кому интересна подписка или лайк: `SharedNetwork::follow` сам решает, что надо положить письмо в ящик. Завтра захочется ещё лога, почты, push и счётчиков трендов, и каждый раз придётся править ту же функцию. Пора развернуть стрелки. Network просто сообщает «произошло вот это» в свою шину событий, а уведомители и прочие подписчики сами решают, что им с этим делать. Какие уведомители работают, выбирается при запуске: в терминале это ящик пользователя всегда и лог в stderr по флагу `--log`.

Вторая половина вехи про терминал. Он разбирает строку и сам решает, кому её отдать, а это тот самый огромный `match`, который растёт с каждой командой. Ядро получает шину команд из главы: обработчик по имени, общий контекст с картой расширений и `IntoResponse`, чтобы обработчик возвращал что ему удобно. Сетевые команды регистрирует ядро, команды сессии регистрирует терминал, а сессия едет в контексте.

## Что сделать

1. **contracts.** Константы с именами команд: `COMMAND_NAMES = ["register", "post", "follow", "like", "feed", "inbox"]` рядом с `Command` и `CHAT_COMMAND_NAMES = ["/login", "/logout", "/whoami", "/msg", "/say", "/history"]` рядом с `ChatCommand`, обе в корне крейта. Разбор не меняется.
2. **social-core, `events/`.** Всё экспортируется из корня крейта.
   - `enum Event` (`Copy`, `PartialEq`, `Debug`): `NewPost { author, post }`, `Liked { by, post, author }`, `Followed { who, whom }`, `MessageSent { from, to }`; `enum Topic { NewPost, Liked, Followed, MessageSent, All }` и `Event::topic()`;
   - `struct SubId(pub u64)`; `EventBus` с `new()`, `subscribe(topic, handler) -> SubId`, `unsubscribe(id) -> bool`, `publish(&event) -> usize` и `subscribers(topic) -> usize`;
   - обработчик это `FnMut(&Event) + Send + 'static`, у каждого своё состояние; id идут с 1 и не выдаются повторно; `publish` вызывает подписчиков темы события и подписчиков `All` в порядке подписки и возвращает, сколько вызвал; `subscribers(t)` считает подписчиков `t` вместе с `All`; шина `Send`.
3. **social-core, Network публикует события.** У `Network` своя шина и методы `subscribe(topic, handler) -> SubId` и `unsubscribe(id) -> bool`. События публикуются только после настоящего изменения: `publish` даёт `NewPost`, новый лайк даёт `Liked` (и свой лайк тоже: это факт, а не уведомление), новая подписка даёт `Followed`, отправленное сообщение даёт `MessageSent`. Повторы, ошибки и регистрация событий не дают. `SharedNetwork` получает такие же `subscribe` и `unsubscribe`; сама событий не создаёт.
4. **social-core, `notify/`: уведомители.**
   - `Notification` получает вариант `Message { from }`, `Display` (`подписка от id 1`, `лайк от id 1 на пост [3]`, `сообщение от id 1`) и `Notification::for_event(&event) -> Option<(UserId, Notification)>`: кому и что. `Followed` уведомляет того, на кого подписались, `Liked` автора (кроме своего лайка), `MessageSent` получателя, `NewPost` никого;
   - `trait Notifier: Send + Sync { fn notify(&self, user: UserId, notification: Notification); }`;
   - реестр `Notifiers` (`Clone`, `Default`): `new()`, `add(Arc<dyn Notifier>)` и `deliver(&event)`, который отдаёт уведомление события каждому уведомителю в порядке добавления;
   - `LogNotifier::new(out: impl Write + Send + 'static)` пишет по строке `[notify] id <кому>: <уведомление>`;
   - ящик из m06 (фоновый поток и mpsc) остаётся в notify/ и становится одним из уведомителей;
   - `SharedNetwork::with_notifiers(network, notifiers) -> io::Result<SharedNetwork>`: добавляет ящик в реестр и подписывает реестр на `Followed`, `Liked` и `MessageSent`. `SharedNetwork::new(network)` это `with_notifiers` с пустым реестром. `inbox` работает как раньше, а команда `inbox` печатает сообщение как `@alice написал тебе`.
5. **social-core, `commands/`: шина команд.** Всё экспортируется из корня крейта.
   - `Response { pub status: u16, pub body: String }`, `Response::new(status, body)`, `is_ok()` для статуса меньше 400;
   - `trait IntoResponse` для `Response`, `String`, `&'static str` (200), `(u16, String)` и `Result<T, E>` (у `Err` с успешным статусом статус становится 500); для `CommandError`: неизвестная команда и неизвестный ник 404, остальные ошибки разбора 400, `Domain` 422, новый вариант `NoExtension(&'static str)` 500 (`в контексте нет <тип>`), тело это `Display` ошибки;
   - `Context`: `new`, `insert` (отдаёт прежнее значение того же типа), `get`, `get_mut` и `require::<T>() -> Result<&T, CommandError>` (нет значения, значит `NoExtension(std::any::type_name::<T>())`). Значения `Any + Send + Sync`, контекст `Send`;
   - `CommandBus`: `new`, `register(name, handler) -> bool` (`true`, если заменил прежний), `dispatch(&mut ctx, line) -> Response` и `names()` по алфавиту. Обработчик это `Fn(&mut Context, &str) -> R + Send + Sync + 'static`, где `R: IntoResponse`; он получает строку без краёв целиком. Имя обработчика это первое слово строки, пустая строка даёт 400 `пустая команда`, незнакомое имя 404 `неизвестная команда: <имя>`. Шина `Send + Sync`;
   - `network_commands::<U, P>(&mut bus)` регистрирует под каждым именем из `COMMAND_NAMES` обработчик, который берёт `SharedNetwork<U, P>` из контекста и исполняет строку через `execute_line`.
6. **tools/repl.**
   - `Terminal::new()` и `Terminal::with_notifiers(notifiers)` собирают шину: сетевые команды ядра и команды сессии под именами из `CHAT_COMMAND_NAMES`. В контексте лежат `SharedNetwork` и `Session` (сначала `Anonymous`);
   - `handle(&mut self, line) -> Response` отдаёт строку шине; `session() -> Option<&Session>` читает сессию из контекста;
   - обработчик сессии делает то же, что в m10: разбор, `step`, действие, и только после успеха новая сессия кладётся в контекст. Он возвращает `Result<String, TerminalError>`, а `IntoResponse` для `SessionError` даёт 401 для `NotLoggedIn` и 409 для `AlreadyLoggedIn` и `NoPeer`;
   - `main.rs`: с флагом `--log` в реестр при запуске добавляется `LogNotifier` в stderr. Успешный ответ печатается как есть, остальные как `ошибка: <тело>`.

## Дерево после вехи

```
backend/
  src/crates/
    contracts/
      src/ chat/mod.rs (+ CHAT_COMMAND_NAMES)  command/mod.rs (+ COMMAND_NAMES)  ...
      tests/
        m07_text_commands.rs  m08_workspace.rs  m10_direct_messages.rs
        m11_events_notifiers.rs   тесты курса
    social-core/
      src/
        lib.rs
        events/mod.rs           Event, Topic, SubId, EventBus
        notify/mod.rs           Notification, Notifier, Notifiers, LogNotifier, ящик
        commands/
          mod.rs                execute, execute_line, network_commands
          bus.rs                CommandBus
          context.rs            Context
          response.rs           Response, IntoResponse
          error.rs              CommandError (+ NoExtension)
        network/  shared/  chat/  domain/  ports/  storage/
      tests/
        m01_user_post.rs ... m10_direct_messages.rs
        m11_events_notifiers.rs   тесты курса
  tools/
    repl/
      src/
        main.rs                 --version, --log, цикл по stdin
        lib.rs  session/mod.rs
        terminal/mod.rs         Terminal на CommandBus, TerminalError
      tests/
        m07_text_commands.rs  m08_workspace.rs
        m10_direct_messages.rs    новая версия от этой вехи
        m11_events_notifiers.rs   тесты курса
    xtask/ ...
```

Корневые файлы (Cargo.toml, Makefile, Makefile.toml, .env, настройки) не меняются.

## Приёмка

- `src/crates/contracts/tests/m11_events_notifiers.rs`: списки имён совпадают с разбором;
- `src/crates/social-core/tests/m11_events_notifiers.rs`: темы событий, порядок вызова, отписка, id без повторов, состояние `FnMut`, шина в другом потоке; Network публикует только настоящие изменения; подписка через `SharedNetwork` из четырёх потоков; `for_event`, тексты уведомлений, свой уведомитель и лог в реестре, ящик как раньше; `IntoResponse` и статусы, контекст как карта по типу (и `Arc<dyn Notifier>` в нём), маршрутизация шины, сетевые команды через шину;
- `tools/repl/tests/m11_events_notifiers.rs`: ответы терминала со статусами, сессия в контексте, уведомители, выбранные при запуске, `repl --log` пишет уведомления в stderr;
- `tools/repl/tests/m10_direct_messages.rs` приходит в новой версии: те же разговоры, но `handle` отдаёт `Response`, а `session()` отдаёт `Option`;
- `cargo make check` зелёный.

Архитектура: новые модули на своих местах, нигде нет `Rc<`, Network не знает про уведомители, SharedNetwork не создаёт событий, контекст на `TypeId`, терминал работает через `CommandBus` и не разбирает `Input` сам, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
printf 'register alice\nregister bob\nfollow bob alice\n/login bob\n/msg alice привет\ninbox alice\n' | cargo run -q -p repl -- --log
```
