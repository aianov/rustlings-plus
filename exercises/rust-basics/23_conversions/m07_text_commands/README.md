# Веха 7. Соцсеть в терминале

Шесть вех соцсеть жила только в тестах. Пора с ней поговорить. Сервера и сети у нас пока нет, зато есть терминал: пишешь `register alice`, соцсеть отвечает `зарегистрирован @alice (id 1)`. Всё, что нужно, уже умеет ядро, не хватает переводчика между строкой текста и вызовом метода.

Такой переводчик в Rust пишется на трейтах преобразований. Строка становится командой через `FromStr` (и тогда работает `line.parse::<Command>()`). Ник становится проверенным `Username` через `TryFrom<&str>`: если `Username` существует, он уже нормализован и правильной длины, и проверять его второй раз незачем. А ошибки двух слоёв (разбор строки и правила соцсети) сводятся в одну `CommandError` через `From`, поэтому `?` работает в любом месте разбора и исполнения.

## Что сделать

1. `src/main.rs` курс положил заготовкой, он уже готов: читает stdin построчно, пропускает пустые строки, печатает ответ команды или `ошибка: <текст>`. Пакет теперь и библиотека, и бинарник с тем же именем `backend`.
2. `src/command/username.rs`: `pub struct Username(String)` с derive `Debug, Clone, PartialEq, Eq`:
   - `impl TryFrom<&str> for Username` с `type Error = DomainError`: те же правила, что у `register` (нормализация, пустой ник, длина);
   - `impl AsRef<str> for Username`: нормализованный ник;
   - `impl Display for Username`: `@alice`.
3. `src/command/error.rs`: ошибки, обе с derive `Debug, Clone, PartialEq, Eq`, `Display` и `std::error::Error`:
   - `pub enum ParseError { Empty, UnknownCommand(String), MissingArgument(&'static str), ExtraArgument(String), BadNumber(String) }`;
   - `pub enum CommandError { Parse(ParseError), Domain(DomainError), UnknownName(String) }`, плюс `impl From<ParseError>` и `impl From<DomainError>`. `Display` у `Parse` и `Domain` печатает внутреннюю ошибку как есть.
4. `src/command/mod.rs`: `pub enum Command` (derive `Debug, Clone, PartialEq, Eq`) и `impl FromStr for Command` с `type Err = CommandError`:

   | строка                         | команда                                        |
   |--------------------------------|------------------------------------------------|
   | `register <ник>`               | `Register(Username)`                           |
   | `post <ник> <текст>`           | `Post { author: Username, text: String }`      |
   | `follow <ник> <на кого>`       | `Follow { who: Username, whom: Username }`     |
   | `like <ник> <id поста>`        | `Like { who: Username, post: PostId }`         |
   | `feed <ник> [сколько]`         | `Feed { who: Username, limit: usize }`, по умолчанию 10 |
   | `inbox <ник>`                  | `Inbox(Username)`                              |

   Слова разделяются пробелами (сколько угодно), пробелы по краям строки не важны. Текст поста это вся остальная строка после ника, пробелы внутри него сохраняются. Ошибки разбора:

   | что не так                              | ошибка                                  | Display                         |
   |-----------------------------------------|-----------------------------------------|---------------------------------|
   | пустая строка                           | `Empty`                                 | `пустая команда`                |
   | первое слово не команда                 | `UnknownCommand("dance")`               | `неизвестная команда: dance`    |
   | нет ника                                | `MissingArgument("ник")`                | `не хватает аргумента: ник`     |
   | нет второго ника в `follow`             | `MissingArgument("на кого")`            | `не хватает аргумента: на кого` |
   | нет текста в `post`                     | `MissingArgument("текст поста")`        | `не хватает аргумента: текст поста` |
   | нет id в `like`                         | `MissingArgument("id поста")`           | `не хватает аргумента: id поста` |
   | слово после последнего аргумента        | `ExtraArgument("bob")`                  | `лишний аргумент: bob`          |
   | id или число постов не число            | `BadNumber("seven")`                    | `не число: seven`               |

   Плохой ник при разборе даёт `CommandError::Domain(...)` прямо из `Username::try_from`.
5. `src/command/execute.rs`: `impl Command { pub fn run<U: UserStore, P: PostStore>(&self, net: &SharedNetwork<U, P>) -> Result<String, CommandError> }`. Ник превращается в id через поиск в сети; незнакомый ник даёт `UnknownName(ник)` с Display `нет пользователя @ghost`. Ответы:

   | команда              | ответ                                                                 |
   |----------------------|-----------------------------------------------------------------------|
   | `register alice`     | `зарегистрирован @alice (id 1)`                                        |
   | `post alice ...`     | `опубликован пост [1]` (видимость всегда `Public`)                     |
   | `follow bob alice`   | `@bob подписан на @alice`, повторно `@bob уже подписан на @alice`      |
   | `like bob 1`         | `@bob лайкнул пост [1]`, повторно `@bob уже лайкал пост [1]`           |
   | `feed bob`           | по строке на пост: `@автор ` и карточка поста, например `@alice [1] Привет (лайков: 1)`; пустая лента: `лента пуста` |
   | `inbox alice`        | по строке на уведомление: `@bob подписался на тебя`, `@bob лайкнул твой пост [1]`; пусто: `уведомлений нет` |

   Строки многострочного ответа склеиваются через `\n`.
6. В `src/lib.rs` выставь наружу `Command`, `CommandError`, `ParseError`, `Username`.
7. Код чистый для Clippy: `cargo clippy --all-targets -- -D warnings` молчит. `unwrap()` в крейте по-прежнему нельзя.

## Дерево после вехи

```
backend/
  Cargo.toml
  src/
    main.rs               из заготовки: цикл по строкам stdin
    lib.rs                + mod command; pub use Command, CommandError, ParseError, Username
    command/
      mod.rs              Command, FromStr
      username.rs         Username: TryFrom<&str>, AsRef<str>, Display
      error.rs            ParseError, CommandError, From
      execute.rs          Command::run
    domain/   mod.rs  error.rs  user.rs  post.rs
    network/  mod.rs  feed.rs  follows.rs  tags.rs
    notify/   mod.rs
    ports/    mod.rs
    shared/   mod.rs
    storage/  mod.rs  users.rs  posts.rs
  tests/                  тесты курса, не правь
    m01_user_post.rs ... m06_shared_threads.rs
    m07_text_commands.rs
```

## Приёмка

- `tests/m07_text_commands.rs`: `Username` проверяет ник на входе; все шесть команд разбираются из строки; каждая ошибка разбора из таблицы; ошибки обоих слоёв встречаются в `CommandError`; команды исполняются с ответами из таблицы; бинарник `backend` получает скрипт в stdin и отвечает ровно теми строками, что в таблице.
- Архитектура: всё из прошлых вех, плюс есть `src/main.rs` и `src/command/mod.rs`; `main.rs` тонкий (не больше 30 строк, разбор живёт в библиотеке); команды работают через `SharedNetwork` и не лезут в `storage`.
- Clippy без предупреждений: курс проверяет его в эталоне, проверь и у себя.

## Как запустить

```
cd backend
cargo test
cargo clippy --all-targets -- -D warnings
cargo run
```

После `cargo run` пиши команды прямо в терминал, выход по Ctrl+D. Или скриптом:

```
printf 'register alice\nregister bob\nfollow bob alice\npost alice Привет #rust\nfeed bob\n' | cargo run -q
```
