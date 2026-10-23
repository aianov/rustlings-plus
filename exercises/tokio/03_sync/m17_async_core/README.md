# Веха 17. Асинхронное ядро

Скоро соцсеть начнёт слушать порт, и к ней придут сотни соединений сразу. Поток на каждое соединение сеть не потянет, а свой пул из m15 умеет только раскладывать задания и ничего не знает ни про таймеры, ни про отмену, ни про ожидание без потока. Пора переезжать на tokio: ядро становится асинхронным фасадом, рассылку делают задачи, тяжёлая работа уходит в `spawn_blocking`, а каждое ожидание получает свой таймаут.

Заодно у соцсети появляются стартовые данные. Команда `import seed.txt` читает файл с регистрациями, подписками и постами и загружает его параллельными задачами, не больше нескольких сразу. Тренды теперь пересчитываются сами раз в минуту по тикеру, а терминал собирает себе рантайм сам, без `#[tokio::main]`.

## Что сделать

1. **Воркспейс.** В `[workspace.dependencies]` корня `tokio = { version = "1", features = ["rt", "rt-multi-thread", "sync", "time", "macros"] }`, `worker-pool` оттуда уходит. Крейт `src/crates/worker-pool` удаляется целиком вместе со своими тестами. social-core: `tokio.workspace = true` в `[dependencies]` и `tokio = { workspace = true, features = ["test-util"] }` в `[dev-dependencies]` (замороженное время в тестах). tools/repl: `tokio.workspace = true`.

2. **contracts.** Команда `import <путь>`: `Command::Import { path: String }`, путь это весь остаток строки без пробелов по краям (в пути бывают пробелы). Без пути это `ParseError::MissingArgument("путь к файлу")`. `COMMAND_NAMES` дописан в конце именем `import` (всего 16).

3. **social-core, `shards/`: шардированная карта.** `Shards<K: Hash + Eq, V>` поверх `Vec<Mutex<HashMap<K, V>>>` (std `Mutex`), шард выбирается хешем ключа (`RandomState::hash_one`, остаток от числа шардов).
   - `Shards::new(count)` (ноль шардов это один), `Default` даёт `DEFAULT_SHARDS = 16`, `count()`, `shard_of(&key) -> usize`;
   - `with(&key, f: impl FnOnce(&mut HashMap<K, V>) -> R) -> R` берёт замок одного шарда на время синхронного `f`. Замыкание синхронное, поэтому `.await` под замком не написать;
   - `get(&key) -> Option<V>` (копия, `V: Clone`), `len()`, `is_empty()`.
   - Экспорт из корня: `social_core::{Shards, DEFAULT_SHARDS}`.

4. **social-core, `notify/`.** Ящик уведомлений больше не держит фоновый поток и канал: это `Shards<UserId, Vec<Notification>>`, `notify` кладёт прямо в шард получателя. В `social-core/src` нет ни одного `thread::`.

5. **social-core, `app/`: фасад `App<U, P>` вместо `SharedNetwork`.** Папка `shared/` уходит. `App` клонируется (`Arc` внутри), это `Send + Sync + 'static`, все клоны делят одно ядро. Методы работают при `U: UserStore + Send + 'static`, `P: PostStore + Send + 'static`.
   - Запуск: `App::start(network, notifiers, config: AppConfig) -> social_core::Result<App<U, P>>`, `App::new(network)` и `App::with_notifiers(network, notifiers)` с `AppConfig::default()`. Запуск берёт текущий рантайм (`Handle::try_current`); вне рантайма это `Error::NoRuntime` (код `internal`, текст `ядро запускается только внутри рантайма tokio`), а не паника. `handle(&self) -> &Handle`, `config(&self) -> &AppConfig`.
   - `AppConfig { op_timeout: Duration, heavy_limit: usize, import_window: usize, trends_every: Duration, shards: usize }` (все поля `pub`, `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`), по умолчанию 5 с, 2, 8, 60 с и 16.
   - Сеть живёт под одним `std::sync::Mutex<Network<U, P>>`, секции короткие и без `.await`. Асинхронные операции: `register`, `publish`, `follow`, `like`, `mute`, `delete_post`, `comment`, `reply`, `send_message` (отдаёт `Message`) с ошибкой `AppError`; синхронные: `read(f)`, `subscribe`, `unsubscribe`, `stats`, `profile`, `trends(n)`. Future каждого async-метода `Send`.
   - Рассылка: `publish` под замком публикует пост и берёт `audience`, отпускает замок и запускает по задаче рантайма на получателя (`Handle::spawn`). Задачи в полёте считает счётчик, `flush(&self) -> Result<(), AppError>` ждёт его нуля на `tokio::sync::Notify` (сначала `enable`, потом проверка, иначе пробуждение потеряется) не дольше `op_timeout`, иначе `AppError::Timeout { op: Op::Flush, after }`. `inbox(user) -> Result<Vec<Notification>, AppError>` сначала делает `flush`. Счётчик уменьшается и при панике уведомителя, и при отмене задачи.
   - `Stats { posts, notifications, trend_refreshes }` (`u64`, `Default`, `Copy`, `Eq`): посты и доставленные уведомления как в m15, плюс число удачных пересчётов трендов.
   - Кэш профилей `profile(id) -> Option<Arc<User>>` живёт в `Shards<UserId, Arc<User>>`: попадание не трогает замок сети, промах копирует пользователя под замком сети и вставляет через `entry().or_insert`, неизвестный id не кэшируется.
   - Тяжёлая работа: `run_heavy<T: Send + 'static>(&self, op: Op, job: impl FnOnce() -> T + Send + 'static) -> Result<T, AppError>`. Разрешение `Semaphore` на `heavy_limit` мест ждётся не дольше `op_timeout` (`acquire_owned`), работа идёт в `spawn_blocking` и держит разрешение до своего конца, даже если её результат перестали ждать; результат ждётся не дольше `op_timeout`. Таймаут это `AppError::Timeout { op, after: op_timeout }`, паника в работе это `AppError::Panicked(op)`.
   - Тренды: `trends(n) -> Vec<Trend>` отдаёт готовую сводку (первые n из `MAX_TRENDS = 100`), `refresh_trends().await` копирует лайки под замком сети и считает топ в `run_heavy(Op::Trends, ..)`. Фоновая задача (запускается в `start`) зовёт `refresh_trends` по `interval_at(сейчас + trends_every, trends_every)` с `MissedTickBehavior::Skip`: после простоя один пересчёт, а не пачка, следующий по сетке. Задача держит только `Weak` на ядро и заканчивается, когда ушёл последний `App`.
   - Импорт: `import(text: String)` и `import_file(path: impl Into<PathBuf>)` -> `Result<Imported, AppError>`, `Imported { users, follows, posts }` (`usize`, `Default`, `Copy`, `Eq`). Файл читается (`std::fs`) и разбирается целиком в `run_heavy(Op::Import, ..)`: строка это текстовая команда `register`, `follow` или `post`, пустые строки пропускаются, ошибка разбора останавливает импорт до первой записи. Потом три волны задач в `JoinSet`, в полёте не больше `import_window`: сначала все `register`, затем подписки, затем посты. Посты одного автора идут одной задачей подряд, в порядке файла (id и время у них растут, как в файле), разные авторы публикуют параллельно. Первая ошибка возвращается сразу, остальные задачи отменяются (дроп набора). Паника в задаче записи это ошибка импорта с номером строки (`JoinError::id`), а не падение процесса. Посты публикуются как обычно, с рассылкой.
   - Экспорт из корня: `App`, `AppConfig`, `AppError`, `ImportError`, `Imported`, `Op`, `Stats`, `MAX_TRENDS`.

6. **social-core, ошибки ядра (`app/error.rs`).**
   - `Op { Import, Trends, Flush, Job(&'static str) }` (`Copy`, `Eq`), `Display`: `импорт`, `пересчёт трендов`, `доставка уведомлений`, имя работы.
   - `AppError { Domain(DomainError), Import(ImportError), Timeout { op, after: Duration }, Panicked(Op), Stopped(Op) }` (`Clone`, `Eq`, `std::error::Error` с `source`): `запрос отклонён` и `импорт остановлен` (причина в `source`), `{op}: превышено время ожидания ({after:?})`, `{op}: задача упала с паникой`, `{op}: ядро останавливается`. `From<DomainError>` и `From<ImportError>`.
   - `ImportError { Read { path: PathBuf, kind: io::ErrorKind }, Parse { line, error: ParseError }, Unsupported { line }, UnknownName { line, name }, Record { line, error: DomainError }, Panicked { line } }`, тексты: `не прочитать <путь>: <kind>`, `строка 3: <ошибка разбора>`, `строка 2: в импорте только register, follow и post`, `строка 2: нет пользователя @ghost`, `строка 20: <доменная ошибка>`, `строка 3: задача импорта упала с паникой`. Строки считаются с единицы, пустые тоже.
   - `social_core::Error` получает `NoRuntime` и `App(AppError)`: `From<AppError>` кладёт `Domain` в `Error::Domain`, остальное в `Error::App` (код `rejected` для импорта, `unavailable` для таймаута, паники и остановки).
   - `CommandError::App(AppError)`: `From<AppError>` кладёт `Domain` в `CommandError::Domain`; `Display` печатает всю цепочку (`report`), статус 422 для импорта и 503 для остального.

7. **social-core, команды и провод.** `execute(&Command, &App)`, `execute_line(line, &App)`, `serve(&App, &Call)` и `serve_line(&App, line)` становятся `async fn`, тексты ответов прежние. `trends` сначала делает `refresh_trends`, поэтому показывает свежие лайки. `stats` делает `flush`. `import <путь>` отвечает `импортировано: пользователей 3, подписок 2, постов 3`. Шина команд остаётся синхронной: обработчик из `network_commands` берёт `App` из контекста и переходит в async через `app.handle().block_on(..)`. Поэтому `dispatch` зовут только из синхронного кода, не из задачи.

8. **tools/repl.** `runtime/mod.rs`: `start() -> repl::Result<Runtime>` собирает `Builder::new_multi_thread()` на `CORE_THREADS = 2` воркера (`repl::CORE_THREADS`), `thread_name("repl-core")`, `enable_all`; ошибка сборки это новый `repl::Error::Runtime(io::Error)` (`не запустился рантайм ядра`). `Terminal` владеет своим рантаймом и запускает `App` внутри него (`runtime.enter()`), команды сессии исполняет через `block_on`, при `Drop` дожидается рассылки (`flush`), поэтому лог `--log` не обрывается. JSON-режим собирает свой рантайм, на каждую строку делает `serve_line` и `flush` в одном `block_on` и только потом печатает push. Никакого `#[tokio::main]`.

9. **Тесты вех переезжают в `tests/journey/`.** Каждый файл `tests/mNN_*.rs` был отдельным тестовым бинарником, и к этой вехе их больше сорока: на каждый уходит своя линковка с tokio и свой первый запуск. Теперь у каждого пакета (contracts, social-core, test-support, tools/repl, tools/xtask) один бинарник `tests/journey/main.rs`, а вехи в нём модули: `mod m01_user_post;` и так далее, файлы лежат рядом, в `tests/journey/`. Веха кладёт туда все прежние тесты и свой `main.rs`; старые `tests/mNN_*.rs` удали (их проверяет archcheck). Твои собственные тесты (`tests/clock.rs`, `tests/common/mod.rs`) остаются где были. Запуск одной вехи: `cargo test -p social-core --test journey m17`.

10. **Старые тесты.** Веха кладёт новые версии тестов, которые ходили в `SharedNetwork`: `social-core/tests/` m06, m07, m10, m11, m12, m13, m14, m15, m16 (теперь `#[tokio::test]` и `App`, каждое ожидание под таймаутом), `contracts/tests/m15_background_fanout.rs` (проверяет начало `COMMAND_NAMES[..15]`), и тесты терминала m07, m10, m11, m13, m15 (дедлайн на работу бинарника 180 с вместо 10: под нагрузкой ОС первый запуск свежего бинарника задерживает на минуту и дольше). Тесты пула `worker-pool` уходят вместе с крейтом.

## Дерево после вехи

```
backend/
  Cargo.toml                        + tokio, без worker-pool
  src/crates/
    worker-pool/                    удалён
    contracts/
      src/command/mod.rs            + import
      tests/journey/
        main.rs                     mod на каждую веху
        m17_async_core.rs           тесты курса
        m15_background_fanout.rs    новая версия
        m07 ... m14                 прежние тесты, переехали
    social-core/
      Cargo.toml                    + tokio, в dev ещё и test-util
      examples/feed_bench.rs        мерит ленту через App и свой рантайм
      src/
        lib.rs                      + App, AppConfig, AppError, ImportError, Imported, Op, Stats,
                                      MAX_TRENDS, Shards, DEFAULT_SHARDS
        shared/                     удалена
        shards/mod.rs               Shards: Vec<Mutex<HashMap>> по хешу ключа
        app/
          mod.rs                    App: запуск, операции, read, inbox, flush, stats, profile
          config.rs                 AppConfig
          error.rs                  Op, AppError, ImportError
          fanout.rs                 задачи рассылки, счётчик в полёте, Notify, Stats
          heavy.rs                  run_heavy: Semaphore, spawn_blocking, таймауты
          import.rs                 import, import_file: разбор, волны в JoinSet
          trends.rs                 сводка трендов, refresh_trends, тикер со Skip
        notify/mod.rs               ящик на Shards, без потока
        error/mod.rs                + NoRuntime, App
        commands/
          mod.rs                    async execute и execute_line, import
          error.rs                  + CommandError::App
          response.rs               + статусы 422 и 503
        api/mod.rs                  async serve и serve_line
        network/posts.rs            + копия лайков для пересчёта трендов
        trends/mod.rs               Trends: Clone
      tests/
        clock.rs, common/mod.rs     твои тесты, на месте
        journey/
          main.rs                   mod на каждую веху
          m17_async_core.rs         тесты курса
          m06 m07 m10 m11 m12 m13 m14 m15 m16   новые версии
          m01 ... m05 m08           прежние тесты, переехали
  tools/repl/
    Cargo.toml                      + tokio
    src/
      lib.rs                        + CORE_THREADS
      runtime/mod.rs                свой Builder
      terminal/mod.rs               рантайм внутри, flush в Drop
      run/mod.rs                    JSON-режим через block_on
      error/mod.rs                  + Error::Runtime
    tests/journey/
      main.rs                       mod на каждую веху
      m17_async_core.rs             тесты курса
      m07 m10 m11 m13 m15           новые версии
      m08 m12 m14                   прежние тесты, переехали
  src/crates/test-support/tests/journey/   main.rs и m16_test_support.rs
  tools/xtask/tests/journey/        main.rs, m09_dev_tooling.rs и m16_test_support.rs
  src/crates/*/tests/mNN_*.rs, tools/*/tests/mNN_*.rs   удалены
```

Остальное дерево не меняется.

## Приёмка

- `src/crates/social-core/tests/journey/m17_async_core.rs`: вне рантайма `NoRuntime`, future всех методов `Send`, шарды раскладывают ключи и не теряют записи восьми писателей, ящики не теряют рассылку десяти авторов, `heavy_limit` держит число одновременных работ, ожидание разрешения и результата обрывается по `op_timeout`, медленная работа держит разрешение до конца, паника работы это `Panicked`, импорт строит сеть, разбор целиком до первой записи, неизвестный ник и доменная ошибка с номером строки, первая ошибка отменяет остальные записи (окно 4), паника записи это ошибка импорта, импорт файла и нет файла, команда `import`, тренды по тикеру и пропуск тиков (на замороженном времени), `flush` с таймаутом, тексты и коды ошибок;
- `src/crates/contracts/tests/journey/m17_async_core.rs`: команда `import` и `COMMAND_NAMES`;
- `tools/repl/tests/journey/m17_async_core.rs`: `import` в терминале и бинарнике (посты одного автора в порядке файла), ошибки импорта не останавливают терминал, `Terminal::new` без чужого рантайма;
- новые версии старых тестов и остальные тесты прошлых вех проходят;
- `cargo make check` зелёный.

Все ожидания в тестах идут с таймаутами, время тикера в тестах заморожено (`start_paused`).

Архитектура: нет `worker-pool` и `shared/`; тесты вех только в `tests/journey/`; в `social-core/src` нет `thread::`, `tokio::sync::Mutex` и `block_on` вне шины команд; нигде в `src/crates` нет `thread::sleep`; ящик на `Shards`, шарды на `Vec<Mutex<HashMap>>`; `Notify` в рассылке, `Semaphore` и `acquire_owned` с `spawn_blocking` в тяжёлой работе, `JoinSet` в импорте, `MissedTickBehavior::Skip` в тикере, свой `enum Op`; рантайм терминала из `Builder::new_multi_thread`, без `#[tokio::main]`; и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
printf 'register alice\nregister bob\nregister carol\nfollow bob alice\nfollow carol alice\npost alice Первый пост #rust\npost carol Привет\n' > seed.txt
printf 'import seed.txt\nfeed bob\ninbox bob\ntrends\nstats\n' | cargo run -q -p repl
```

Терминал импортирует семь записей тремя волнами задач (пользователи, подписки, посты по авторам), показывает ленту bob и его уведомление о посте alice.
