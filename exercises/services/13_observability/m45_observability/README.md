# Веха 45. Наблюдаемость

Соцсеть пишет в лог как придётся: `eprintln!("{NAME}: релей outbox встал: {error}")` в одном сервисе, `println!` с адресами в другом, строка доступа шлюза в третьем формате. Найти путь одного запроса через шлюз, user-service и notify-service можно только глазами и по времени. request-id есть, но в половине строк его нет, а между gRPC и NATS он теряется. Сколько запросов в секунду и сколько из них падает, никто не знает, пока не пожалуется пользователь. И однажды в лог шлюза уже попадал заголовок `Authorization` целиком.

Эта веха делает соцсеть наблюдаемой. Все сервисы пишут через `tracing`: событие с полями, span на запрос через `Instrument`, уровень из `RUST_LOG`, а свой слой печатает каждое событие одной строкой JSON вместе с полями всех span'ов вокруг. Трейс по W3C `traceparent` идёт от клиента через шлюз (заголовок HTTP или `meta` конверта WebSocket) в gRPC metadata и в заголовки NATS, а потребитель события продолжает тот же трейс. Метрики живут в service-runtime (счётчики, gauge, накопительные гистограммы) и отдаются в формате Prometheus на `/metrics` admin-порта. RED (запросы, ошибки, время) считается одним слоем: у шлюза на каждый HTTP-маршрут с шаблоном пути в метке, у каждого gRPC-сервиса на каждый метод. Токены, пароли и `Authorization` в лог не попадают.

Всё собирается из главы 13_observability: задачи с svobs1 по svobs16 и проект svobs_quiz.

## Что сделать

1. **Окружение воркспейса**: `tracing = "0.1"`, `tracing-subscriber` (без умолчаний, с `registry`, `std`, `env-filter`) в `[workspace.dependencies]`; `RUST_LOG=info` в `.env.example` и `.env`. Тестам нужны dev-зависимости: `serde_json` у service-runtime и `tracing-subscriber` у шлюза.

2. **service-runtime**:
   - `telemetry/`: `JsonLayer::new(service, write(level, line))` и `JsonLayer::standard(service)` (INFO и ниже в stdout, WARN и ERROR в stderr). Слой хранит поля span'а в его расширениях (`on_new_span`, `on_record`), а событие пишет одной строкой: `ts` (мс), `level`, `service`, `target`, `message`, поля события, поля всех span'ов от корня (ближний перекрывает дальний, поле события перекрывает span), `spans` (имена через `>`). Поля с именами `ts`, `level`, `service`, `target` из span'ов пишутся как `field_<имя>`. `escape_json` экранирует кавычку, обратную косую, `\n`, `\r`, `\t`, управляющие символы и U+2028/U+2029. `SECRET_FIELDS` (authorization, token, password, cookie, secret, access_token, refresh_token и всё на `_<одно из них>`) заменяются на `HIDDEN`, а `Bearer <что угодно>` в любом тексте обрезается до `Bearer [скрыто]`. `filter()` это `EnvFilter` из `RUST_LOG` (по умолчанию `info`), `init_logging(service)` ставит глобальный подписчик и отвечает, получилось ли;
   - `metrics/`: `Metrics` (`inc`, `set`, `add`, `observe`, `render`), глобальный `metrics()`, `BUCKETS` с 5 мс по 10 с. Ряд определяется именем и набором меток (порядок меток не важен), у семейства один `# HELP` и один `# TYPE`, значения меток экранируются, гистограмма накопительная с `le="+Inf"`, `_sum` и `_count`;
   - `propagation/`: `TRACEPARENT`, `TraceContext { trace_id, span_id, sampled }` (`fresh`, строгий `parse` по W3C: версия не `ff`, 32 и 16 шестнадцатеричных символов в нижнем регистре, не все нули; `child`, `header`, `continue_from(Option<&str>)`), task-local `with_trace(trace, future)` и `current_trace()`.

3. **admin**: `GET /metrics` отдаёт `metrics().render()` с `METRICS_TYPE` (`text/plain; version=0.0.4; charset=utf-8`).

4. **grpc-kit**: `Stamp::in_trace(trace)` и `trace()`; `Stamp::pass("traceparent", ..)` берёт ребёнка пришедшего трейса; `Stamp::follow` читает `traceparent` из metadata; перехватчик кладёт `traceparent` (свой или текущий task-local). `AccessService` открывает span `grpc` (`server`, `method`, `request_id`, `trace_id`, `user`), выполняет вызов внутри `with_trace(..).instrument(span)` и считает `GRPC_REQUESTS` (`grpc_server_requests_total` по `service`, `method`, `code`) и `GRPC_DURATION` (`grpc_server_request_duration_seconds`). `LogSink::stderr()` теперь пишет событие tracing с полями.

5. **nats-engine**: у `Meta` поле `traceparent` (по умолчанию ребёнок текущего трейса), `with_traceparent`, `trace()`; заголовок `traceparent` пишется и читается (кривой отбрасывается). `open_feed` и `serve` выполняют обработчик внутри span'а (`event` или `request`) с продолжением трейса из заголовка.

6. **http-engine**: `Router::into_measured(observer)` сообщает `Served { method, template, status, elapsed }` после каждого ответа; `template` это шаблон маршрута (`/users/:username`), HEAD считается по GET, непойманное это `UNMATCHED`; `Router::template(method, path)`.

7. **Сервисы**: `service_runtime::init_logging(NAME)` первой строкой `main`; ни одного `println!` и `eprintln!` (включая пример `feed_bench`: там `writeln!`), вместо них `tracing::info!`, `warn!`, `error!`.

8. **Шлюз** (`controller/observe.rs`): слой `trace_requests()` сразу после `request_ids()`: продолжает `traceparent` клиента или начинает новый, кладёт его в запрос и в ответ, выполняет запрос в span'е `http` (`method`, `path`, `request_id`, `trace_id`); `stamp()` берёт трейс из заголовка; RED HTTP через `into_measured` (`http_requests_total` по `method`, `route`, `status`, `http_request_duration_seconds`), прокси `/auth/*` считается маршрутом `/auth/*`; каждый запрос WebSocket получает свой трейс (или ребёнка `traceparent` из `meta` конверта), идёт в span'е `ws` и считается в `ws_requests_total` (`method`, `outcome`) и `ws_request_duration_seconds`; строка доступа это событие `target: "access"`.

Клиент курса приходит готовым: `trace on` печатает после каждой команды `request-id` ответа, `trace off` выключает.

## Дерево после вехи

```
backend/src/
  crates/service-runtime/src/
    telemetry/mod.rs      JsonLayer, init_logging, escape_json, SECRET_FIELDS
    metrics/mod.rs        Metrics, metrics(), BUCKETS
    propagation/mod.rs    TraceContext, with_trace, current_trace
  microservices/api-gateway/src/controller/observe.rs
```

Остальное дерево прежнее.

## Приёмка

Тестам нужен NATS курса с JetStream на `127.0.0.1:54222` (адрес из `RUSTLINGS_NATS_URL`).

- `service-runtime/tests/journey/m45_observability.rs`: строка JSON с полями события и span'а; секреты скрыты; экранирование держит событие в одной строке; формат Prometheus с накопительной гистограммой; строгий разбор `traceparent`; трейс живёт в задаче.
- `http-engine/tests/journey/m45_observability.rs`: в метке шаблон маршрута, непойманное это `unmatched`.
- `admin/tests/journey/m45_observability.rs`: `/metrics` на admin-порту.
- `grpc-kit/tests/journey/m45_observability.rs`: `traceparent` в metadata, продолжение у сервера, текущий трейс доходит до следующего вызова.
- `api-gateway/tests/journey/m45_observability.rs`: один трейс клиента в строке доступа шлюза и в строке user-service, по request-id из ответа находится тот же путь, `meta.traceparent` из WebSocket доходит до user-service, RED по шаблону маршрута, WebSocket и gRPC на `/metrics`, токен и пароль в логи не попали.
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву; `.enter()` нигде в `src`; `println!` нигде в сервисах; `init_logging(NAME)` в каждом `main`; `EnvFilter::try_from_env`, свой `Layer`, `SECRET_FIELDS`, накопительная гистограмма, task-local трейс, `/metrics`, `traceparent` в gRPC и NATS, продолжение трейса потребителем, `into_measured` у шлюза; зависимости по списку; и все договорённости прошлых вех.

## Как запустить

NATS курса должен работать: `127.0.0.1:54222`.

```
cd backend
cargo make check
cargo make demo
```

В клиенте: `register alice correct-horse`, `login alice correct-horse`, `trace on`, `find alice`. Клиент напечатает `request-id` ответа. В логах шлюза и user-service найдётся строка с этим `request_id` и общим `trace_id`, по которому видно, куда запрос ходил. `curl -s http://127.0.0.1:9880/metrics` покажет `http_requests_total` и `ws_requests_total` шлюза, а admin-порт user-service покажет `grpc_server_requests_total` по методам.
