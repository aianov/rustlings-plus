# service-runtime: основа для всех сервисов

Соцсеть растёт, сервисов будет много, и каждый из них должен стартовать, жить и останавливаться
одинаково: читать конфиг с проверкой, ждать базу, отвечать на `/health` и `/ready`, не терять
запросы на остановке и не молчать, когда фоновая задача упала. Копировать это в каждый сервис
нельзя, поэтому всё собирается в один крейт `service-runtime`, а сервис `echo-service` показывает, как он
подключается. Ты уже написал все кирпичи в задачах главы, теперь складываешь их в библиотеку.

## Что уже есть

- Воркспейс с двумя пакетами: `src/crates/service-runtime` и `src/microservices/echo-service`.
- `echo-service/src/main.rs`: короткий `main`, он зовёт `echo_service::run()`.
- Приёмочные тесты обоих пакетов (`tests/acceptance.rs`), их править нельзя.
- Пустые `lib.rs` в обоих пакетах.

## Что сделать в `service-runtime`

Модули объявляются в `lib.rs`, каждый в своём файле. Имена и сигнатуры такие же, как в задачах главы.

1. `config`: `ConfigError { Missing(String), Invalid { name, value } }` с `Display` (называет
   переменную), `parse_or<T: FromStr>(name, Option<String>, default) -> Result<T, ConfigError>`,
   `required(name, Option<String>) -> Result<String, ConfigError>` (пустое значение считается
   отсутствующим), `parse_dotenv(&str) -> Result<HashMap<String, String>, DotenvError { line }>`,
   `layered(&HashMap, real) -> impl Fn(&str) -> Option<String>` (окружение важнее файла) и
   `Secret<T>` (`new`, `expose`, `Debug` это `Secret(***)`, `Display` это `***`).
2. `signals`: `ShutdownListener::new()` и `wait()` с `Signal::{Term, Int}`.
3. `lifecycle`: `Lifecycle::new(Vec<JoinHandle<()>>)`, `state()`, `shutdown(self, grace, drain_deadline)
   -> Outcome { drained }`, публичные `State { ready, accepting, in_flight }`, `begin_request(&Arc<State>)`.
4. `health`: трейт `Check`, `Health::new(check_timeout)`, `register`, `begin_shutdown`, `live`,
   асинхронный `ready() -> Report { ready, failures }`.
5. `startup`: `Policy { interval, total }`, `StartupError`, `wait_for(name, &policy, probe) -> Result<u32, _>`.
6. `supervisor`: `TaskSpec { name, critical, factory }`, `RestartPolicy { max_restarts, pause }`,
   `SupervisorError::CriticalFailed { name, last_error }`, `run(specs, policy, shutdown)`.

Крейт не читает окружение сам: никаких `env::var` и `set_var` внутри него.

## Что сделать в `echo-service`

В `lib.rs`:

- `Settings` (с `Debug`): `port` (по умолчанию 7000, ноль недопустим), `grace` (`SHUTDOWN_GRACE_MS`, 100),
  `drain_deadline` (`DRAIN_DEADLINE_MS`, 2000), `database_addr` (`DATABASE_ADDR`, обязательный),
  `api_token: Secret<String>` (`API_TOKEN`, обязательный).
- `Settings::from_source(get) -> Result<Settings, Vec<ConfigError>>`: все ошибки сразу, в порядке полей.
- `Settings::load(env_file: &str, real) -> Result<Settings, Vec<ConfigError>>`: текст `.env` плюс окружение,
  окружение важнее. Битая строка `.env` это одна ошибка, в имени которой стоит `.env`.
- `pub async fn run() -> Result<(), String>`: прочитать `.env` и окружение, дождаться базы
  (`DATABASE_ADDR` это `host:port`, проба это TCP-подключение), открыть порт и отвечать на `/health` и
  `/ready`, по SIGTERM или Ctrl+C остановиться в нужном порядке. Тестами это не покрыто, но
  `cargo run -p echo-service` должен работать.

## Критерии приёмки

`cargo test --workspace` проходит и `cargo clippy --workspace --all-targets -- -D warnings` чист.
Тесты проверяют: разбор `.env` и слои конфига, секреты, готовность и живость, ожидание зависимостей
с дедлайном, порядок остановки (по времени, на замороженных часах), падение критичной задачи,
отсутствие чтения окружения в библиотеке и короткий `main.rs`.

## Подсказки

README главы: «Конфиг», «Остановка», «live и ready», «Старт», «Супервизор». Задачи `svcfg1`..`svcfg16`
уже содержат почти весь код: твоя работа в том, чтобы разложить его по модулям и согласовать типы.
