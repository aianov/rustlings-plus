# Веха 34. Сервис как процесс

Слои разложены, но процесс по-прежнему живёт как учебная программа. Адреса читаются через `env::var` в трёх разных местах, опечатка в переменной молча превращается в умолчание, Ctrl-C обрывает запросы на полуслове, а оркестратору не у кого спросить, жив ли сервис и можно ли слать ему трафик. Эта веха делает из каждого сервиса долгоживущий процесс, которым можно управлять: настройки читаются в одном месте и проверяются все разом, SIGTERM запускает остановку по шагам, у каждого сервиса появляется admin-порт с `/health` и `/ready`, на старте сервис ждёт соседей с дедлайном, а фоновые задачи живут под надзором.

Собирается всё из главы 01_config_runtime: упражнения с svcfg1 по svcfg16 и svcfg_runtime. Снаружи соцсеть та же: клиент, шлюз и gRPC-контракт не меняются.

## Что сделать

1. **service-runtime** (зависимость по-прежнему одна, `tokio`):
   - `config/`. `Source` это пары ключ и значение: `new`, `from_pairs`, `with`, `process()` (окружение процесса), `from_dotenv(path)` и `load(path)` (файл `.env` снизу, окружение процесса сверху; нет файла, значит пусто), `over(top)`, `get` (пустое значение считается незаданным).
     `parse_dotenv(text)` понимает комментарии, `export`, кавычки и хвостовой ` #`, а кривые строки возвращает все разом как `Problem::Dotenv { line }` без текста строки: там может оказаться пароль.
     `Fields` (`source.fields()`): `parse`, `optional`, `required`, `text`, `addr` (хост:порт), `millis`, `flag`, `list`, `secret`, `reject`, `reject_secret`, `distinct_addrs` (два слушателя на одном порту, порт 0 не в счёт), `finish() -> Result<(), ConfigError>`. После ошибки чтение возвращает умолчание и идёт дальше, поэтому `finish` называет все плохие переменные сразу.
     `Problem { Missing, Invalid { key, value, reason }, Dotenv { line }, Unreadable }`, `ConfigError { problems }` с `keys()` и `Display` по строке на ошибку.
     `Secret<T>`: `new`, `expose`, `into_inner`, `Debug` печатает `Secret(***)`, `Display` печатает `***`.
   - `signals/`: `Signals::listen()` сразу подписывается на SIGTERM и SIGINT (вне unix только Ctrl-C), `next() -> Signal { Terminate, Interrupt }`, `shutdown(trail) -> CancellationToken`: первый сигнал отменяет токен, повторные только пишутся в журнал. `process::exit` в проекте запрещён, а остановка и так ограничена дедлайнами.
   - `trail/`: `Trail` (`quiet`, `stderr(имя)`, `new(f)`, `note`, `lines`) это журнал жизни процесса: что ждём, какой шаг остановки, какая задача выключена.
   - `health/`: трейт `Check` (`run() -> CheckFuture`; замыкание `|| async { .. }` тоже проверка), `Health` (`new(limit)`, `default` с `CHECK_LIMIT` 500 мс, `register(name, check)`, `names`, `stop`, `is_stopping`, `stopped`, `report`, `wait(&Policy)`).
     `report()` запускает проверки одновременно, каждую со своим лимитом (зависшая это `timeout`), сохраняет порядок регистрации, паника проверки это провал под её именем, а на остановке отвечает «не готов», никого не спрашивая.
     `Report { ready, stopping, probes }` с `failing()` и `summary()`, `Probe { name, outcome, took }`, `StartupError { attempts, limit, failing }`.
   - `startup/`: `Policy { interval, total }` (по умолчанию 200 мс и 10 с), `wait_for(&policy, probe) -> Result<u32, GaveUp<E>>`: попытки с паузой и одним общим дедлайном, каждой попытке не меньше `interval` на ответ.
   - `background/`: `Background::new(trail)`, `spawn(name, Criticality, Restart, make)` поверх прежнего `supervise`, `run(token) -> Result<(), Fatal>`. Некритичная задача после последнего перезапуска выключается с записью в журнал, критичная останавливает остальные и возвращает `Fatal { name, attempts }`, отмена токена отменяет и задачи.
   - `lifecycle/`: `Timing { grace, drain, wait }`: `Default` для тестов (100 мс, 2 с, без ожидания) и `Timing::read(fields)` для боя (`SHUTDOWN_GRACE_MS` 1000, `SHUTDOWN_DRAIN_MS` 2000, `STARTUP_WAIT_MS` 10000, `STARTUP_RETRY_MS` 200).
     `Lifecycle::new(&signal, health, trail).timing(timing)` держит токены `stopping` (дочерний от сигнала), `closing`, `background`, `finished` и записывает шаги `Step { NotReady, Closed, Drained, Abandoned, BackgroundStopped, Stopped }`.
     `run(wait, serving, background) -> Result<(), Halt<E>>`, `Halt { Startup, Serve, Fatal }`. Порядок: фон работает с первой секунды; если задан `wait`, ждать готовности всех проверок; работать до сигнала, падения сервера или смерти критичной задачи; потом `ready=false`, пауза `grace`, `closing`, запросы в полёте не дольше `drain`, остановка фона, `finished`. Не дождался готовности или сигнал пришёл раньше: сервер так и не начинает принимать запросы, а шаги `NotReady`, `Closed`, `Drained` не пишутся.

2. **src/crates/admin** (новый крейт: `contracts`, `http-engine`, `service-runtime`, `tokio`, в тестах `social-cli`): `router(health)` с `GET /health` (200 и `{"status":"live"}`, без единой проверки) и `GET /ready` (200 или 503 с `ReadyView`), `serve(listener, health, stop)`, `Admin::start(Option<TcpListener>, &lifecycle)` и `stop()`, `admin_config()`, `ready_view(&report)`. Крейт добавь в `[workspace.dependencies]` первым делом: его тесты уже лежат в `src/crates/admin/tests`, и без `Cargo.toml` не загрузится весь workspace.

3. **contracts**: `LiveView { status }`, `ReadyView { ready, stopping, checks }` с `failing()` и `summary()`, `CheckView { name, ok, reason, millis }`.

4. **journal**: `Journal::health() -> JournalHealth`; `check()` отдаёт ошибку последней неудачной записи, пока следующая запись не пройдёт.

5. **grpc-kit**: `Tls.key` это `Secret<Vec<u8>>`; `Tls::from_env` и `dir_from_env` удалены, каталог сертификатов приходит из конфига; `ACCESS_LOG_ENV`. `Readiness` помнит причину (`status(label)`, `NOT_YET`), `Readiness::supervised(upstreams, &mut background)` кладёт наблюдателей в фон как критичные задачи с `WATCH_RESTART`, `register(&health)` делает из каждого соседа проверку. `unready_on_stop::<S>(reporter, health)` переводит gRPC-здоровье в NOT_SERVING вместе с `/ready`.

6. **proto**: `Endpoint::addr()` удалён, `Endpoint { env, default }` остаётся: адрес читает конфиг сервиса.

7. **Каждый сервис**:
   - `common/config.rs`: `NAME`, `ADMIN_ENV` и `DEFAULT_ADMIN` (шлюз 9880, auth 9881, users 9882, posts 9883, chat 9884), `Config::from_source(&Source) -> Result<Config, ConfigError>` (свой адрес, admin, соседи, журнал, `CERTS_DIR`, `Timing::read`, `ACCESS_LOG`, `Trail::stderr(NAME)`, `distinct_addrs`) и `Config::lifecycle(&shutdown)`. `Config::default()` остаётся для тестов: тихий журнал, короткая пауза, без ожидания. `from_env` удалён.
   - `common/error.rs`: варианты `Config`, `Startup`, `Background` и `From<Halt<..>>`.
   - `bootstrap/`: прежний `run(..)` без admin-порта (им пользуются тесты) и новый `serve(.., admin, config, shutdown)`. Оба регистрируют проверки (`journal` у сервисов с журналом, `user-service` у post, chat и auth, четыре сервиса у шлюза), собирают фон и `Lifecycle`, сервер перестаёт принимать по `closing`. Потоки ленты и чата тоже заканчиваются по `closing`. У user-service `build(&config, &health)`.
   - `main.rs` не длиннее 60 строк: `Signals::listen()` первой строкой, `Config::from_source(&Source::load(".env")?)`, `Tls::load(&config.certs, NAME)`, `TcpListener::bind` на каждый адрес, строка о запуске с admin-адресом, `serve`. Ошибка печатается через `report` и даёт код 1.
   - api-gateway: `Config` с `http_addr`, `admin`, `certs`, `max_connections` (`GATEWAY_MAX_CONNECTIONS`, не ноль), `budget` (`GATEWAY_BUDGET_MS`, не ноль), `Services::read(fields)`; публичный `/health` шлюза остаётся прежним.
   - auth-service: `Config` с `addr`, `http_addr` и `admin`, все три разные.
   - У user-service и post-service в `[dev-dependencies]` `certgen`: тесты запускают настоящий бинарь с сертификатами.

8. **tools/social-cli** приходит новой версией: `await_ready(admin, limit, out)` ждёт `/ready` шлюза и называет неготовые сервисы, флаги `--admin` и `--no-wait`, переменная `GATEWAY_ADMIN_ADDR`.

9. **Корень**: `admin` в `[workspace.dependencies]`; в `.env.example` и `.env` admin-адреса пяти сервисов, `SHUTDOWN_GRACE_MS`, `SHUTDOWN_DRAIN_MS`, `STARTUP_WAIT_MS`, `STARTUP_RETRY_MS`, `ACCESS_LOG`; `demo` в `Makefile.toml` и `Makefile` гасит сервисы `kill -TERM` и ждёт их.

## Дерево после вехи

```
backend/
  .env.example                              admin-адреса, SHUTDOWN_*, STARTUP_*, ACCESS_LOG
  src/crates/
    service-runtime/src/config/             Source, Fields, ConfigError, Secret, parse_dotenv
    service-runtime/src/signals/ trail/     Signals, Trail
    service-runtime/src/health/ startup/    Health, Check, Policy, wait_for
    service-runtime/src/background/         Background, Criticality, Fatal
    service-runtime/src/lifecycle/          Lifecycle, Timing, Step, Halt
    admin/src/routes/ server/               /health и /ready на http-engine
    contracts/src/health/                   LiveView, ReadyView, CheckView
    journal/src/health/                     JournalHealth
  src/microservices/*/src/main.rs           сигналы, конфиг, сертификаты, bind, serve
  src/microservices/*/src/common/config.rs  Config::from_source
  tools/social-cli/src/ready/               await_ready (новая версия клиента)
```

## Приёмка

- `service-runtime/tests/journey/m34_service_runtime.rs`: разбор `.env` (комментарии, export, кавычки, номера кривых строк без их текста), окружение главнее файла, все ошибки конфига разом, порт на двоих, секрет не попадает ни в `Debug`, ни в ошибку; проверки идут одновременно с лимитом, на остановке не опрашиваются, паника это провал под именем; ожидание с паузой и общим дедлайном (время заморожено, число попыток точное); некритичная задача выключается, критичная роняет фон, отмена не оставляет сирот; порядок остановки, обрыв по `drain`, смерть критичной задачи, неудачный старт без приёма запросов, сигнал во время ожидания; SIGTERM и SIGINT приходят значением, а не убивают процесс.
- `admin`: `/health` отвечает сразу при зависшей проверке, `/ready` с причинами и 503, на остановке 503 при живом `/health`, порт закрывается по токену.
- `user-service`: `from_source` с умолчаниями и всеми переменными, все ошибки разом, `Debug` конфига без ключа; настоящий бинарь отвечает на `/health` и `/ready`, по SIGTERM выходит с кодом 0 и шагами по порядку, с кривым конфигом падает сразу, до сертификатов и портов, читает `.env` из рабочей папки (окружение главнее), Ctrl-C тоже штатная остановка.
- `post-service`: `/ready` следит за user-service, а `/health` нет; бинарь ждёт соседа, `/ready` называет его, после дедлайна процесс падает с причиной; SIGTERM во время ожидания выходит сразу и чисто.
- `chat-service` и `auth-service`: конфиг, `/ready` без соседа, три адреса auth-service.
- `api-gateway`: конфиг шлюза, `/ready` называет упавший сервис, клиент курса ждёт готовности и называет неготовый, бинарь без сервисов сдаётся после дедлайна и называет все четыре.
- новые версии `support.rs` шлюза (`Gateway::with_admin`, `halt`) и `m33_layers.rs` user-service (`build(&config, &health)`);
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву, крейт `admin` в workspace и с узким набором зависимостей, `main.rs` не длиннее 60 строк с `Signals::listen`, `Config::from_source`, `Source::load`, `Tls::load` и `TcpListener::bind`, нигде в сервисах нет `env::var`, `ctrl_c` и `tokio::signal`, нет `env::var` в proto и grpc-kit, `from_source` с `finish`, `distinct_addrs` и `Timing::read` у каждого сервиса, `Admin::start`, `serve`, `closing` и `lifecycle.run` в каждом bootstrap, проверки `journal` и `user-service`, ключ сервиса в `Secret`, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

`demo` поднимает пять сервисов, а клиент сначала ждёт `/ready` шлюза: пока соседи поднимаются, он пишет, кого ждёт, а потом «шлюз готов». Во втором окне `curl -s 127.0.0.1:9883/ready` покажет проверки post-service. Останови user-service (`kill -TERM` его процесса): `/ready` post-service и шлюза назовут его, а `/health` останется 200. При выходе из клиента сервисы получают SIGTERM, и в выводе каждого видны шаги остановки. Ошибись в `.env` (например, `SHUTDOWN_GRACE_MS=секунда` и `USERS_GRPC_ADDR=localhost`): сервис не стартует и называет обе переменные разом.
