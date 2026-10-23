# Веха 8. Воркспейс

Соцсеть выросла из одного пакета. В `backend` живут вперемешку правила соцсети, разбор строк терминала и сам терминал, а скоро рядом встанут сервер, клиент, генератор нагрузки и десяток сервисов. Если всё это останется одним крейтом, каждая правка будет пересобирать всё, а любой модуль сможет залезть в любой другой. Поэтому `backend` становится воркспейсом: виртуальный корень-оглавление, несколько пакетов с ясными стрелками между ними, общие версии и общие правила.

Делим так. Внизу `contracts`: id, проверенный ник и команды терминала, то, о чём договариваются все. Он ни от кого не зависит. Над ним `social-core`, ядро соцсети из прошлых вех. Сверху `tools/repl`, терминал, который теперь умеет ещё и `--version`.

```
tools/repl  ──>  social-core  ──>  contracts
```

## Что сделать

1. Корневой `Cargo.toml` виртуальный (без `[package]`):
   - `[workspace]` с `resolver = "3"` и членами-масками `"src/crates/*"` и `"tools/*"`;
   - `[workspace.package]`: `version = "0.1.0"`, `edition = "2024"`, `rust-version = "1.88"`, `publish = false`;
   - `[workspace.dependencies]`: `contracts` и `social-core` по путям от корня;
   - `[workspace.lints.rust]` с `unsafe_code = "forbid"` и `[workspace.lints.clippy]` с `unwrap_used = "deny"`;
   - `[profile.release]` с `lto = true` и `codegen-units = 1`.
2. У каждого пакета: поля `version`, `edition`, `rust-version`, `publish` наследуются из корня (`version.workspace = true` и так далее), зависимости записаны как `имя.workspace = true`, есть `[lints]` с `workspace = true`.
3. `src/crates/contracts` (библиотека, без зависимостей):
   - `ids/mod.rs`: `UserId` и `PostId` переезжают сюда из ядра без изменений;
   - `username/mod.rs`: `normalize_name`, `MAX_NAME_LEN`, `Username` и новая ошибка `pub enum NameError { Empty, TooLong }` (derive `Debug, Clone, Copy, PartialEq, Eq`, `Display` с теми же фразами, что у `EmptyName` и `NameTooLong`, `std::error::Error`). `Username::try_from` теперь возвращает `NameError`: про `DomainError` contracts не знает. Добавь `impl From<Username> for String`;
   - `command/`: `Command` и `ParseError`. `FromStr for Command` теперь с `type Err = ParseError`, а плохой ник даёт новый вариант `ParseError::BadName(NameError)` (Display печатает ошибку ника как есть);
   - фасад `lib.rs`: `Command`, `DEFAULT_FEED_LIMIT`, `ParseError`, `PostId`, `UserId`, `MAX_NAME_LEN`, `NameError`, `Username`, `normalize_name`.
4. `src/crates/social-core` (библиотека, зависит от `contracts`): все модули ядра из прошлых вех.
   - Ядро берёт ids, ник и его нормализацию из `contracts` и выставляет их в своём фасаде как раньше (`social_core::UserId` и `contracts::UserId` это один и тот же тип). Правило ника живёт в одном месте: `validate_name` ядра опирается на `Username::try_from`, а `impl From<NameError> for DomainError` переводит `Empty` в `EmptyName`, `TooLong` в `NameTooLong`;
   - исполнение команд переезжает в модуль `commands/`. Метод `Command::run` больше не написать: нельзя добавлять методы чужому типу. Вместо него функции `pub fn execute(command: &Command, net: &SharedNetwork<U, P>) -> Result<String, CommandError>` и `pub fn execute_line(line: &str, net: &SharedNetwork<U, P>) -> Result<String, CommandError>` (разобрать строку и исполнить). `CommandError` остаётся в ядре: в ней встречаются `ParseError` из contracts и `DomainError` из ядра. Ответы те же, что в вехе 7;
   - фасад как раньше плюс `CommandError`, `execute`, `execute_line`.
5. `tools/repl` (бинарник `repl`, зависит от `social-core`): цикл по строкам stdin из старого `main.rs`. Флаг `--version` печатает строку `REPL_VERSION` и выходит, не читая stdin. Эту строку собирает `build.rs`: `repl <версия пакета> (<профиль>)`, например `repl 0.1.0 (debug)`. Версию и профиль скрипт читает из переменных `CARGO_PKG_VERSION` и `PROFILE` и отдаёт в компиляцию директивой `cargo::rustc-env=REPL_VERSION=...`, а `main.rs` берёт её через `env!("REPL_VERSION")`.
6. Старого пакета `backend` больше нет: удали `src/lib.rs`, `src/main.rs`, модули в `src/` и `tests/` в корне. Тесты прошлых вех курс уже разложил по `tests/` новых пакетов.
7. Код чистый для `cargo clippy --workspace --all-targets -- -D warnings` с новыми правилами: `unwrap()` теперь запрещён и в тестах, там вместо него `expect("что пошло не так")`.

## Дерево после вехи

```
backend/
  Cargo.toml                    виртуальный корень
  Cargo.lock                    один на весь воркспейс (создаст cargo)
  src/
    crates/
      contracts/
        Cargo.toml
        src/
          lib.rs                фасад
          ids/mod.rs            UserId, PostId
          username/mod.rs       normalize_name, MAX_NAME_LEN, NameError, Username
          command/
            mod.rs              Command, FromStr
            error.rs            ParseError
        tests/                  тесты курса: m07, m08
      social-core/
        Cargo.toml
        src/
          lib.rs                фасад
          commands/
            mod.rs              execute, execute_line
            error.rs            CommandError
          domain/  network/  notify/  ports/  shared/  storage/
        tests/                  тесты курса: m01 ... m08
  tools/
    repl/
      Cargo.toml
      build.rs                  REPL_VERSION
      src/main.rs               цикл по stdin, --version
      tests/                    тесты курса: m07, m08
```

## Приёмка

- `src/crates/social-core/tests/m01 ... m06`: прежнее поведение ядра, импорты теперь из `social_core`.
- `src/crates/contracts/tests/m07_text_commands.rs`: ник и разбор команд, все ошибки разбора, включая `BadName`.
- `src/crates/social-core/tests/m07_text_commands.rs`: `CommandError` и исполнение команд через `execute_line`.
- `tools/repl/tests/m07_text_commands.rs`: бинарник `repl` отвечает на скрипт в stdin теми же строками, что раньше `backend`.
- `m08_workspace.rs` в трёх пакетах: ids и правила ника живут в contracts и это те же типы в ядре; `repl --version` печатает строку из build.rs; версия наследуется из корня.
- Архитектура: корень виртуальный с resolver 3, все пакеты в members, общие поля, зависимости и lints через корень, `contracts` без зависимостей, ядро не знает про repl, в `src/` корня только `crates/`, фасады без типов, и все договорённости прошлых вех на новых путях.

## Как запустить

```
cd backend
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p repl -- --version
cargo run -p repl
```
