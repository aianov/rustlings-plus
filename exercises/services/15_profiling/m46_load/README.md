# Веха 46. Своя нагрузка и узкие места

Соцсеть проверена тестами, у неё есть логи и метрики, но никто не знает, сколько она держит. Сто пользователей? Тысячу? Что сломается первым: шлюз, user-service или брокер? Метрики RED покажут, что p99 растёт, но не скажут, почему: может, все соединения шлюза дерутся за один замок карты сессий, может, кто-то считает argon2 прямо в задаче рантайма, а может, очередь задач tokio растёт быстрее, чем воркеры её разбирают.

Эта веха даёт соцсети свой генератор нагрузки и первые найденные им узкие места. `tools/loadgen` гоняет сценарии через тот же шлюз, с которым говорит клиент: вход по HTTP, пост, лента и сообщение по WebSocket. Две модели: закрытая (N пользователей по кругу) и открытая (прибытия по расписанию, задержка считается от запланированного момента, поэтому очередь видна). Задержки копит логарифмическая гистограмма со слиянием, отчёт сравнивается с прошлым прогоном, а ступени удваивают частоту, пока p99 и доля ошибок в пределах. На `/metrics` появляются метрики рантайма, а сторож долгих poll предупреждает о блокирующем коде. Найденное чинится: карта сессий шлюза шардирована, пуш кодируется, только если есть кому его отдать, argon2 остаётся в `spawn_blocking` и проверяется правилом.

Всё собирается из глав 14_load_testing и 15_profiling: задачи с svload1 по svload11, svprof1 по svprof7 и проекты svload_quiz и svprof_quiz.

## Что сделать

1. **service-runtime**:
   - `runtime_stats/`: `record_runtime(&Metrics) -> bool` пишет gauge `tokio_alive_tasks`, `tokio_global_queue_depth`, `tokio_workers` из `Handle::try_current()` (вне рантайма `false`);
   - `watchdog/`: `watch_polls(name, future)` и `watch_polls_with(name, limit, future)` (`SLOW_POLL` = 10 мс): poll дольше предела это `tokio_slow_polls_total{task}` и предупреждение в лог. Долгое ожидание таймера долгим poll не считается.

2. **admin**: `/metrics` вызывает `record_runtime` перед выдачей.

3. **grpc-kit**: обработчик gRPC идёт под `watch_polls("grpc", ..)`.

4. **Шлюз**: `Registry` держит `SHARDS` = 16 карт под своими замками, `Registry::shard_of(user)`; запись, выход, доставка трогают один шард, а `users`, `connections`, `seats`, `revoke` проходят все; `deliver` и `deliver_stored` кодируют пуш, только если есть получатели; запросы HTTP и WebSocket под сторожем (`http-request`, `ws-request`); `Registry` и `SHARDS` экспортируются.

5. **`tools/loadgen`** (член воркспейса; ни reqwest, ни hyper, ни tungstenite, ни hdrhistogram: клиенты это `Web` и `Ws` клиента курса):
   - `histogram/`: `Histogram` с `SUB_BITS` = 7: значения до 128 точно, дальше корзина по старшим восьми битам (ошибка меньше процента); `record`, `merge`, `quantile`, `count`, `min`, `max`, `mean`;
   - `model/`: `Sample { step, took, ok }`, `Hit` (замыкание: номер исполнителя → будущее со списком `Sample`), `Closed { users, duration }` и `run_closed`, `Open { rate, duration, workers }` и `run_open` (прибытия строго по расписанию, исполнитель берётся из пула, ожидание в очереди добавляется к первому шагу), `Stats` (`add`, `merge`, `report`);
   - `scenario/`: `Step { Login, Post, Feed, Message }`, `Scenario::parse("login,post,feed,message")` и `Scenario::mixed()`, `User::sign_up(gateway)` (регистрация, вход и WebSocket до начала замера), `User::step(step) -> Sample`, `stack_target(gateway, users, scenario) -> Hit` (каждый пишет сообщения следующему по кругу);
   - `report/`: `Report` (запросы, ошибки, доля, в секунду, p50, p90, p99, максимум, `StepReport` по шагам) с `render`, `to_json`, `from_json`; `compare(before, after)` (изменение в процентах и вывод: хуже, лучше или в пределах шума);
   - `ramp/`: `Ramp { start, factor, steps, p99, errors }`, `ramp(plan, run) -> RampOutcome { reports, best, stop }`, `Stop { Latency, Errors, OutOfSteps }`;
   - `cli/` и `main.rs`: флаги `--gateway`, `--model`, `--users`, `--rate`, `--duration`, `--scenario`, `--ramp`, `--p99-ms`, `--max-errors`, `--save`, `--compare` (`USAGE`); адрес по умолчанию из `GATEWAY_HTTP_ADDR`;
   - Makefile.toml: `cargo make load` (закрытая модель на 10 с, сравнение с `target/load-last.json` и сохранение туда же).

## Дерево после вехи

```
backend/
  src/crates/service-runtime/src/
    runtime_stats/mod.rs  record_runtime
    watchdog/mod.rs       watch_polls, SLOW_POLL
  tools/loadgen/
    Cargo.toml
    src/
      lib.rs
      main.rs
      histogram/mod.rs
      model/mod.rs
      scenario/mod.rs
      report/mod.rs
      ramp/mod.rs
      cli/mod.rs
```

Остальное дерево прежнее.

## Приёмка

Тестам нужен NATS курса с JetStream на `127.0.0.1:54222` (адрес из `RUSTLINGS_NATS_URL`).

- `loadgen/tests/journey/m46_load.rs`: квантили в пределах процента и точные малые значения; слияние как одна гистограмма; закрытая модель держит пользователей занятыми; открытая держит расписание и показывает очередь; ступени останавливаются на первом превышении p99 и доли ошибок; отчёт читается из JSON и сравнивается; флаги; короткая нагрузка по стеку в процессе без ошибок и с p99 меньше секунды по всем четырём шагам.
- `service-runtime/tests/journey/m46_load.rs`: тяжёлый poll замечен, ожидание таймера нет; gauge рантайма в метриках.
- `api-gateway/tests/journey/m46_load.rs`: карта сессий шардирована и остаётся согласованной под параллельной записью.
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: `tools/loadgen` в воркспейсе и без запрещённых зависимостей; логарифмическая гистограмма со слиянием, задержка от запланированного момента, останов ступеней, сравнение отчётов, задача `load`; метрики рантайма и сторож; `watch_polls` у gRPC; шарды карты сессий; argon2 в `spawn_blocking`; и все договорённости прошлых вех.

## Как запустить

NATS курса должен работать: `127.0.0.1:54222`.

```
cd backend
cargo make check
cargo make demo
```

Во втором терминале `cargo make load`: генератор заведёт десять пользователей и десять секунд будет писать посты, читать ленту и слать сообщения через тот же шлюз, с которым говорит клиент. Отчёт покажет запросы в секунду, p50, p90, p99 и ошибки по шагам, а второй запуск сравнит себя с первым. `cargo run -p loadgen --release -- --ramp --p99-ms 100` найдёт частоту, которую стенд держит с p99 до 100 мс, а `curl -s http://127.0.0.1:9880/metrics | grep tokio_` покажет живые задачи и глубину очереди рантайма.
