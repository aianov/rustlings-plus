# Веха 23. Консоль задач

Внутри ядра уже живёт десяток фоновых задач: тикер и актор трендов, супервизоры и воркеры доставки, писатель журнала, а импорт на минуту поднимает ещё пачку. Пока всё работает, их не видно. Но однажды `stats` повиснет, и останется гадать, кто кого ждёт. В tokio есть tokio-console, у нас его нет, поэтому пишем свою маленькую консоль: каждая задача ядра регистрируется под именем, дамп показывает, кто жив, сколько раз его опрашивали и сколько он уже ждёт, а сторож запоминает poll, который держал поток слишком долго.

Тот же реестр ловит утечки: после сеанса живых задач ровно столько, сколько было до него, а после `close` ноль.

## Что сделать

1. **service-runtime, консоль** (`src/console/`, файлы `mod.rs` и `traced.rs`).
   - `Console` (Clone, общий реестр): `Console::new()` (порог долгого poll `DEFAULT_LONG_POLL`, 100 мс), `Console::with_threshold(порог)`, `threshold()`.
   - `traced(name: &'static str, future) -> Traced<F>`: регистрирует future сразу, ещё до первого poll, где бы его потом ни опрашивали (tokio::spawn, JoinSet, супервизор).
   - `Traced<F>: Future`: на каждом poll отмечает `TaskState::Polling`, считает poll, после poll ставит `TaskState::Idle` и время конца poll, меряет длину poll (`std::time::Instant`) и отдаёт её сторожу; `Drop` снимает задачу с учёта: завершилась (`finished`), брошена до конца (`cancelled`: abort, дроп JoinSet) или упала посреди poll (`panicked`).
   - `spawn(name, future) -> JoinHandle` это `tokio::spawn(traced(..))`.
   - `tasks() -> Vec<TaskInfo>` (живые, по возрастанию id), `TaskInfo { id, name, state, polls, since, longest_poll }`: id с единицы в порядке регистрации, `since` время в текущем состоянии по часам tokio (`tokio::time::Instant`, чтобы тесты на замороженном времени были точными). Display: `#1 yielder: ждёт 600 мс, poll 4` или `#3 hog: в poll 40 мс, poll 2`.
   - `alive()`, `counts() -> TaskCounts { alive, spawned, finished, cancelled, panicked }`.
   - Сторож: poll дольше порога попадает в `long_polls() -> Vec<LongPoll { id, name, took }>` (последние `KEPT_LONG_POLLS` = 32).
   - `dump()`: строка `задач живых 2, запущено 3, завершено 1, брошено 0, паник 0`, затем по строке на живую задачу, затем `долгий poll: #1 hog 60 мс` на каждый долгий poll.

2. **service-runtime, стадии.** У `Shutdown` своя консоль: `Shutdown::with_console(console)`, `console() -> &Console` (`Shutdown::new()` берёт `Console::new()`). `Stage` получает поле `console` и метод `spawn(name, task)`: задача идёт и в трекер стадии, и в консоль. Прежние `TaskTracker::spawn` и `supervise` не меняются.

3. **social-core.** `AppConfig::long_poll` (100 мс) это порог сторожа, `App::console() -> &Console`. Все фоновые задачи ядра запускаются через консоль под именами: `trends-ticker` (стадия sources), `fanout-supervisor` и `fanout-worker` (стадия handlers, воркер это `console.traced` внутри `make` супервизора), `trends-actor` (trends), `journal-writer` (journal), `import` (задачи JoinSet импорта через `traced`). Голых `tokio::spawn`, `tokio::task::spawn` и `stage.tasks.spawn` в ядре нет.

4. **Команда `tasks`.** `Command::Tasks` (`tasks`, без аргументов) дописана в конец `COMMAND_NAMES`, метод `admin.tasks` в конец `COMMAND_METHODS`; обработчик сначала дожидается доставки (`flush`, как `stats`), потом отвечает дампом консоли.

## Дерево после вехи

```
backend/
  src/crates/
    service-runtime/src/
      lib.rs                        + Console, Traced, TaskInfo, TaskState, TaskCounts, LongPoll
      console/
        mod.rs                      Console, реестр, дамп, сторож
        traced.rs                   Traced
      shutdown/mod.rs               + консоль у Shutdown, Stage::spawn
    social-core/src/
      app/
        mod.rs                      консоль из long_poll, тикер и актор через stage.spawn, console()
        fanout.rs                   супервизоры и воркеры доставки через консоль
        journal.rs                  journal-writer через stage.spawn
        import.rs                   задачи импорта через traced
        config.rs                   + long_poll
      commands/
        admin.rs                    + tasks
        table.rs                    + admin.tasks
    contracts/src/command/
      mod.rs                        + Command::Tasks, COMMAND_NAMES
      method.rs                     + admin.tasks
  */tests/journey/m23_task_console.rs  тесты курса в contracts, service-runtime, social-core и tools/repl
```

Остальное дерево не меняется.

## Приёмка

- `service-runtime/tests/journey/m23_task_console.rs`: запись живёт, пока жива задача; poll и время ожидания считаются на замороженном времени; abort и паника считаются отдельно; `traced` работает в JoinSet и для future, который так и не опросили; точный формат дампа; сторож ловит poll, который крутится без `.await`; `Stage::spawn` кладёт задачу в консоль;
- `social-core/tests/journey/m23_task_console.rs`: каждая фоновая задача ядра видна под своим именем, после `close` живых нет; сеанс с импортом (и импортом с ошибкой), постами, карточкой и живой лентой не оставляет лишних задач; уведомитель, который крутится без `.await` в воркере доставки, виден как долгий poll в дампе команды `tasks`;
- `contracts/tests/journey/m23_task_console.rs`: разбор `tasks` и метод `admin.tasks`; новая версия `m19_async_ports.rs` проверяет начало списка команд;
- `tools/repl/tests/journey/m23_task_console.rs`: `tasks` в терминале печатает дамп свежего ядра;
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву, консоль в `service-runtime/src/console` без новых зависимостей, задачи ядра только через `stage.spawn` и `traced`, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
printf 'register alice\nregister bob\nfollow bob alice\npost alice Кто тут работает?\ntasks\n' | cargo run -q -p repl
```

`tasks` печатает десять живых задач ядра: у каждой имя, сколько её опрашивали и сколько она ждёт следующего пробуждения.
