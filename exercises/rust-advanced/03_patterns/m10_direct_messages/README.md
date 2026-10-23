# Веха 10. Личные сообщения

Посты читают все, а пользователи хотят поговорить вдвоём. В терминале соцсети теперь можно войти под своим ником и переписываться: `/login alice`, `/msg bob привет`, дальше `/say как дела?` уходит тому же bob, а `/history bob` показывает переписку. Терминал впервые помнит что-то между строками: кто вошёл и с кем открыт диалог. Значит, у него появляется состояние, и на каждую команду надо честно решить, можно ли её сейчас выполнить. Это ровно автомат на `(состояние, событие)` из главы: явные переходы, ни одной ветки «как-нибудь», отказ с понятной причиной.

Старые команды (`register`, `post`, `follow`, `like`, `feed`, `inbox`) работают как раньше и в любом состоянии сессии: в них автор назван явно. Команды сессии начинаются с `/` и берут автора из сессии.

## Что сделать

1. **contracts: команды сессии.** Новый модуль `chat/` с `ChatCommand` и разбором `impl FromStr for ChatCommand` (ошибка `ParseError`):

   ```rust
   pub enum ChatCommand {
       Login(Username),
       Logout,
       Whoami,
       Msg { to: Username, text: String },
       Say(String),
       History { with: Username, limit: usize },
   }
   ```

   | строка | результат |
   |---|---|
   | `/login <ник>` | `Login(ник)` |
   | `/logout`, `/whoami` | `Logout`, `Whoami` |
   | `/msg <ник> <слово>...` | `Msg { to, text }` |
   | `/say <слово>...` | `Say(text)` |
   | `/history <ник> [сколько]` | `History { with, limit }`, без числа `DEFAULT_HISTORY_LIMIT` (20) |

   - слова разделены любыми пробелами, края строки не важны; слова текста склеиваются одним пробелом (`/say ну   что` даёт `"ну что"`);
   - строка без слов это `Empty`; первое слово без `/` даёт `UnknownCommand(слово)`; неизвестная команда даёт `UnknownCommand("/имя")`, одинокий `/` даёт `UnknownCommand("/")`; имена чувствительны к регистру;
   - не хватает слова: `MissingArgument("ник")` для `/login`, `MissingArgument("кому")` для `/msg`, `MissingArgument("текст сообщения")` для `/msg <ник>` и `/say`, `MissingArgument("с кем")` для `/history`;
   - лишнее слово: `ExtraArgument(первое лишнее)` у `/login`, `/logout`, `/whoami`, `/history`;
   - ник проверяется как раньше (`BadName`); число истории: не число или не влезает в `usize` даёт `BadNumber(как введено)`, а число вне `1..=MAX_HISTORY_LIMIT` (100) даёт новый вариант `ParseError::LimitOutOfRange(n)` с текстом `лимит истории от 1 до 100, а не 0`;
   - порядок ошибок: неизвестная команда, потом форма (не хватает или лишнее), потом содержимое (ник, потом число);
   - разбор построен на срезовых образцах по словам (`words.as_slice()`), хвост текста берётся образцом `text @ ..`, ранние выходы написаны через let-else. Паниковать нельзя ни на каком вводе.
2. **contracts: одна точка входа.** Модуль `input/` с `enum Input { Command(Command), Chat(ChatCommand) }` и `FromStr`: строка, которая после пробелов начинается с `/`, разбирается как `ChatCommand`, остальные как старый `Command`. Разбор `Command` не меняется. В `Command` добавь два варианта, которые не пишутся текстом, а собираются из сессии: `SendMessage { from: Username, to: Username, text: String }` и `History { who: Username, with: Username, limit: usize }`. В `src/crates/contracts/src/**` нет ни одной ветки `_ =>`.
3. **social-core: модуль `chat/`.** Тип `Message` с методами `sender()`, `recipient()` (оба `UserId`) и `text()`, константа `MAX_MESSAGE_LEN = 500`, хранилище переписок в памяти. Оба экспортируются из корня крейта.
   - `Network::send_message(&mut self, sender, recipient, text) -> Result<&Message, DomainError>`: оба пользователя существуют (`UnknownUser`), себе нельзя (`SelfMessage`), текст обрезается по краям, пустой даёт `EmptyMessage`, длиннее `MAX_MESSAGE_LEN` символов даёт `MessageTooLong`. Ошибки проверяются в этом порядке, и отклонённое сообщение никуда не попадает;
   - `Network::history(&self, user, peer, limit) -> Result<&[Message], DomainError>`: последние `limit` сообщений переписки этих двоих от старых к новым. История общая: `history(alice, bob, ..)` и `history(bob, alice, ..)` одинаковы. Нет переписки, значит пустой срез; неизвестный пользователь даёт `UnknownUser`;
   - `SharedNetwork::send_message(&self, sender, recipient, text) -> Result<Message, DomainError>` отдаёт копию сохранённого сообщения; историю читают через `read`;
   - тексты ошибок: `нельзя написать самому себе`, `сообщение не может быть пустым`, `сообщение длиннее 500 символов`;
   - `execute` исполняет новые команды: `SendMessage` отвечает `@alice → @bob: текст` (текст в том виде, в каком сохранён), `History` печатает по строке на сообщение `@автор: текст` или `переписки пока нет`. Незнакомый ник даёт `CommandError::UnknownName`.
4. **tools/repl: автомат сессии.** У терминала появляется библиотека: `src/lib.rs` (только `mod` и `pub use`), `src/session/` и `src/terminal/`. Пакет зависит от `contracts` и `social-core`.

   ```rust
   pub enum Session { Anonymous, LoggedIn { user: Username }, Chatting { user: Username, peer: Username } }
   pub enum Action { Login(Username), Logout(Username), Whoami, Run(Command) }
   pub enum SessionError { NotLoggedIn, AlreadyLoggedIn(Username), NoPeer }

   impl Session {
       pub fn step(&self, command: ChatCommand) -> Result<(Session, Action), SessionError>;
   }
   ```

   | состояние | команда | новое состояние, действие |
   |---|---|---|
   | Anonymous | `Login(u)` | `LoggedIn { u }`, `Login(u)` |
   | Anonymous | `Whoami` | то же, `Whoami` |
   | Anonymous | `Logout`, `Msg`, `Say`, `History` | отказ `NotLoggedIn` |
   | LoggedIn, Chatting | `Login` | отказ `AlreadyLoggedIn(u)` |
   | LoggedIn, Chatting | `Logout` | `Anonymous`, `Logout(u)` |
   | LoggedIn, Chatting | `Whoami` | то же, `Whoami` |
   | LoggedIn, Chatting | `Msg { to, text }` | `Chatting { u, peer: to }`, `Run(SendMessage { from: u, to, text })` |
   | LoggedIn | `Say` | отказ `NoPeer` |
   | Chatting | `Say(text)` | то же, `Run(SendMessage { from: u, to: peer, text })` |
   | LoggedIn, Chatting | `History { with, limit }` | то же, `Run(History { who: u, with, limit })` |

   - автомат чистый: про сеть он не знает, в `session/` нет `social_core`, нет `_ =>` и нет веток вида `(состояние, _) =>`;
   - `Display` у `Session`: `вход не выполнен`, `ты @alice`, `ты @alice, диалог с @bob`. У `SessionError` (и `std::error::Error`): `сначала войди: /login <ник>`, `сессия @alice уже открыта, сначала /logout`, `диалог не открыт, начни его: /msg <ник> <текст>`.
5. **tools/repl: терминал.** `Terminal::new() -> io::Result<Terminal>` держит `SharedNetwork` в памяти и `Session` (сначала `Anonymous`); `session()` отдаёт текущую сессию; `handle(&mut self, line) -> Result<String, TerminalError>` разбирает `Input`:
   - `Command` исполняется через `execute`, как раньше;
   - `ChatCommand` идёт в `step`, потом терминал исполняет действие: `Login` проверяет, что ник зарегистрирован (иначе `CommandError::UnknownName`), и отвечает новой сессией (`ты @alice`); `Logout` отвечает `до встречи, @alice`; `Whoami` отвечает сессией; `Run` исполняется через `execute`;
   - новая сессия записывается, только если и переход, и действие прошли. Неудачный `/login ghost` оставляет `Anonymous`, неудачный `/msg ghost ...` не открывает и не переключает диалог;
   - `enum TerminalError { Command(CommandError), Session(SessionError) }`, ошибка разбора приходит как `Command(CommandError::Parse(..))`, `Display` печатает вложенную ошибку.

   `main.rs` остаётся коротким: `--version`, цикл по строкам stdin, пустые пропускает, печатает ответ или `ошибка: ...`.

## Дерево после вехи

```
backend/
  Cargo.toml  Makefile  Makefile.toml  .env.example  .env  .cargo/config.toml
  rust-toolchain.toml  rustfmt.toml  clippy.toml
  src/crates/
    contracts/
      src/
        lib.rs                  mod и pub use
        chat/mod.rs             ChatCommand, DEFAULT_HISTORY_LIMIT, MAX_HISTORY_LIMIT, разбор
        command/mod.rs          Command (+ SendMessage, History), разбор старых команд
        command/error.rs        ParseError (+ LimitOutOfRange)
        input/mod.rs            Input
        ids/mod.rs  username/mod.rs
      tests/
        m07_text_commands.rs  m08_workspace.rs
        m10_direct_messages.rs  тесты курса
    social-core/
      src/
        lib.rs
        chat/mod.rs             Message, MAX_MESSAGE_LEN, переписки
        commands/  domain/  network/  notify/  ports/  shared/  storage/
      tests/
        m01_user_post.rs ... m08_workspace.rs
        m10_direct_messages.rs  тесты курса
  tools/
    repl/
      Cargo.toml                contracts и social-core
      build.rs
      src/
        main.rs                 --version и цикл по stdin
        lib.rs                  mod и pub use
        session/mod.rs          Session, Action, SessionError, step
        terminal/mod.rs         Terminal, TerminalError
      tests/
        m07_text_commands.rs  m08_workspace.rs
        m10_direct_messages.rs  тесты курса
    xtask/ ...
```

## Приёмка

- `src/crates/contracts/tests/m10_direct_messages.rs`: разбор каждой команды сессии, все ошибки и их порядок, `Input` различает два языка, старый разбор `/msg` не знает, странный ввод не роняет разбор;
- `src/crates/social-core/tests/m10_direct_messages.rs`: одна история на двоих, лимит берёт последние, переписки не смешиваются, правила сообщений (символы, а не байты), сообщения из четырёх потоков через `SharedNetwork`, ответы `execute`;
- `tools/repl/tests/m10_direct_messages.rs`: вся таблица переходов `step`, тексты сессии и ошибок, разговор через `Terminal`, отказы не меняют сессию, бинарник держит одну сессию на весь stdin;
- тесты прошлых вех проходят как раньше, `cargo make check` зелёный (формат, clippy без предупреждений, тесты).

Архитектура: новые модули на своих местах, в contracts и в автомате нет `_ =>`, разбор на срезовых образцах и let-else, автомат не трогает сеть, терминал зависит только от contracts и social-core, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
printf 'register alice\nregister bob\n/login alice\n/msg bob привет\n/say как дела?\n/logout\n/login bob\n/history alice\n' | cargo run -q -p repl
```
