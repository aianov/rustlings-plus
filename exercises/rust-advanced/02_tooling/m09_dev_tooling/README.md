# Веха 9. Окружение разработчика

В воркспейсе три пакета, и уже пора запоминать: тесты это `cargo test --workspace`, линтер это `cargo clippy --workspace --all-targets -- -D warnings`, формат проверяется `cargo fmt --all --check`, терминал запускается `cargo run -p repl`. Через месяц пакетов будет десять, и каждый новый человек начнёт с вопроса «а как у вас всё запустить?». Ответ должен лежать в репозитории, а не в голове. Поэтому соцсеть получает короткие команды, закреплённый компилятор, общие правила форматирования и скрипты на Rust, которые сами создают заготовку будущего сервиса.

С этой вехи курс проверяет соцсеть не просто тестами, а командой `cargo make check`: формат, clippy без предупреждений и тесты. Как в настоящем CI.

## Что сделать

1. `Makefile` в корне: цели `build`, `run`, `test`, `fmt`, `lint`, `check`, все в `.PHONY`.
   - `fmt` это `cargo fmt --all --check`, `lint` это `cargo clippy --workspace --all-targets -- -D warnings`, `test` это `cargo test --workspace`, `build` это `cargo build --workspace`;
   - `check` зависит от `fmt lint test` и выполняет их в этом порядке, останавливаясь на первой ошибке;
   - `run` запускает `cargo run --quiet -p $(SERVICE)`, а `SERVICE ?= repl`: `make run SERVICE=x` и `SERVICE=x make run` работают оба, аргумент сильнее окружения.
2. `Makefile.toml` для cargo-make:
   - `env_files` подключает `.env` с `defaults_only = true`;
   - в `[config]` стоит `default_to_workspace = false`;
   - задачи `fmt`, `lint`, `test` с теми же командами, `check` с `dependencies = ["fmt", "lint", "test"]` и задача `run` (`cargo run --quiet -p ${SERVICE}`). У задач с именами встроенных (`test`, `check` и прочих) поставь `clear = true`, иначе cargo make check после твоих шагов выполнит ещё и встроенный `cargo check`.
3. `.env.example` в репозитории: ключи `SERVICE=repl` и `RUST_BACKTRACE=1`. Свой `.env` сделай копией: `cp .env.example .env`. Без него cargo make откажется работать (`Unable to load env file`).
4. `rust-toolchain.toml`: `channel = "1.88.0"`, `components = ["rustfmt", "clippy"]`, `profile = "minimal"`. `rustfmt.toml`: `max_width = 100` и `use_field_init_shorthand = true`. `clippy.toml`: `disallowed-methods` с `std::process::exit` (из main возвращаем `ExitCode`).
5. `tools/xtask`: бинарник `xtask`, член воркспейса с теми же наследованием и lints, без зависимостей. Алиас в `.cargo/config.toml`: `xtask = "run --quiet --package xtask --"`.
   - корень воркспейса xtask ищет сам: ближайшая папка выше текущей (включая её), где `Cargo.toml` содержит строку `[workspace]`. Так `cargo xtask` работает из любой папки проекта;
   - `cargo xtask check`: в корне запускает три шага `fmt --all --check`, `clippy --workspace --all-targets -- -D warnings`, `test --workspace` тем cargo, путь к которому лежит в переменной `CARGO` (её задаёт сам cargo). На первом упавшем шаге останавливается, называет его и завершается с ошибкой;
   - `cargo xtask new-service <имя>`: создаёт `src/microservices/<имя>/` с `Cargo.toml` (наследование полей из корня, `[dependencies]` пустые, `[lints]` с `workspace = true`) и `src/main.rs`, который печатает `<имя> started`. Если в `members` корня ещё нет маски `"src/microservices/*"`, команда дописывает её в строку `members = [...]`: пустая маска ломает воркспейс, поэтому заранее её не держим. Имя: маленькие латинские буквы, цифры и одиночные дефисы, начинается с буквы, не кончается дефисом. Плохое имя или уже существующий сервис дают ошибку, и на диске ничего не меняется;
   - без аргументов и с неизвестной командой печатает справку, где названы `check` и `new-service`, и завершается с ненулевым кодом.
6. `milestone.toml` курса теперь проверяет веху командой `cargo make check`. Значит, весь код, тесты курса и твои тоже, должен проходить `cargo fmt --all --check` по твоему `rustfmt.toml`. Один раз прогони `cargo fmt --all`: тесты курса уже отформатированы.

## Дерево после вехи

```
backend/
  Cargo.toml
  Makefile                      build, run, test, fmt, lint, check
  Makefile.toml                 то же для cargo make, .env, run
  .env.example                  SERVICE, RUST_BACKTRACE
  .env                          твоя копия .env.example
  .cargo/config.toml            alias xtask
  rust-toolchain.toml           1.88.0, rustfmt, clippy
  rustfmt.toml
  clippy.toml
  src/crates/contracts/ ...
  src/crates/social-core/ ...
  tools/
    repl/ ...
    xtask/
      Cargo.toml
      src/
        main.rs                 разбор аргументов
        root.rs                 поиск корня воркспейса
        check.rs                три шага проверки
        service.rs              new-service
      tests/
        m09_dev_tooling.rs      тесты курса
```

## Приёмка

`tools/xtask/tests/m09_dev_tooling.rs`. Чтобы не гонять настоящие проверки внутри проверки, тесты подкладывают поддельный `cargo`, который записывает, что его просили сделать:

- `make check` и `cargo make check` выполняют ровно `fmt`, `lint`, `test` по порядку и останавливаются на первой ошибке; встроенного `cargo check` нет;
- `make run` выбирает сервис по правилам `?=`, цели работают рядом с одноимёнными файлами;
- `cargo make run` берёт `SERVICE` из `.env`, но переменная оболочки сильнее;
- в `.env.example` есть все ключи, в `.env` есть все ключи из `.env.example`; компилятор закреплён точной версией;
- `xtask` без команды печатает справку; `xtask check` находит корень из вложенной папки, запускает три шага оттуда и останавливается на упавшем;
- `xtask new-service` создаёт заготовку, регистрирует маску один раз, не перезаписывает существующее и отвергает плохие имена;
- в копии воркспейса `cargo xtask new-service chat-service` (через алиас) и `cargo run -p chat-service` печатают `chat-service started`;
- пробные пакеты с твоими `rustfmt.toml` и `clippy.toml` показывают, что правила действуют.

Архитектура: файлы окружения на месте, `tools/xtask` в воркспейсе с общими полями и lints, нужные строки в Makefile, Makefile.toml, `.cargo/config.toml`, `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`, и все договорённости прошлых вех.

## Как запустить

Нужен cargo-make (`cargo install cargo-make`, один раз).

```
cd backend
cp .env.example .env
cargo fmt --all
make check
cargo make check
cargo xtask check
make run
```
