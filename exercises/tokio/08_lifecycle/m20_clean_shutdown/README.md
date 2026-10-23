# Веха 20. Чистая остановка

Соцсеть научилась работать, но не научилась уходить. Когда терминал закрывается, рантайм просто умирает вместе со всеми задачами: тикер трендов обрывается посреди пересчёта, неполная пачка уведомлений так и остаётся в памяти, а задача доставки, упавшая на кривом уведомителе, молча пропадает, и с этой минуты подписчики ничего не получают. Для сервиса, который перезапускают на каждом релизе, так нельзя.

Эта веха учит ядро жить и умирать по правилам. Появляется свой крейт `service-runtime`: токен отмены с деревом детей, учёт задач, супервизор и остановка по стадиям, всё без `tokio-util`. Тренды переезжают в настоящий актор, уведомления разносит пул воркеров пачками, и у каждого получателя порядок сохраняется. А `App::close(deadline).await` гасит ядро по течению данных и добивает тех, кто не успел к сроку.

## Что сделать

1. **Новый крейт `src/crates/service-runtime`** (пакет `service-runtime`, библиотека `service_runtime`, общие поля и `[lints]` из воркспейса, зависит только от `tokio`, без комментариев). Корень объявляет его в `[workspace.dependencies]`, ядро подключает в `[dependencies]`. Свои тесты крейт гоняет на замороженном времени, поэтому в его `[dev-dependencies]` `tokio = { workspace = true, features = ["test-util"] }`: иначе `cargo test -p service-runtime` в одиночку не соберётся.
   - `token/`: `CancellationToken` (`Clone`, `Default`, `Send + Sync`) на `Arc<watch::Sender<bool>>`: `new()`, `child()` (отмена родителя отменяет всех потомков, отмена ребёнка родителя не трогает), `cancel()` (через `send_replace`, чтобы сигнал не терялся без слушателей), `is_cancelled()`, `cancelled()` (отдаёт `Pin<Box<dyn Future<Output = ()> + Send + '_>>`: ждёт свой флаг или отмену любого предка, поздний слушатель не зависает).
   - `tracker/`: `TaskTracker` (`Clone`, `Default`): `spawn(future) -> JoinHandle<T>`, `len()`, `is_empty()`, `close()`, `is_closed()`, `abort_all() -> usize` (сколько задач было живо), `wait().await` (закрыт и пуст). Задача уходит из учёта и когда закончилась, и когда её отменили (guard в `Drop`), и даже если закончилась раньше, чем `spawn` успел её записать.
   - `supervisor/`: `supervise(make, token, Restart { pause, max_restarts }) -> Outcome`: запускает `make()` задачей, после паники ждёт `pause` и запускает снова, после `max_restarts` перезапусков сдаётся. `Outcome { Finished { restarts }, GaveUp { attempts }, Cancelled { restarts } }`. По отмене токена останавливает ребёнка (`abort`) и ждёт его. Ребёнок не переживает супервизора: если отменили саму задачу супервизора, guard в `Drop` отменяет и ребёнка.
   - `shutdown/`: `Shutdown::new()`, `stage(name) -> Stage { name, token, tasks }` (токен стадии это ребёнок общего корня), `token()` (корень), `close(deadline).await -> ShutdownReport`: стадии гасятся строго по порядку объявления (отмена токена, `close` учёта, ожидание), у всей остановки один дедлайн, кто к нему не успел, тот получает `abort_all`. `ShutdownReport { stages: Vec<StageReport { name, aborted }> }`, `aborted()` и `order()`. После всех стадий отменяется корень.

2. **social-core, уведомители пачками.** `Notifier::notify_batch(&self, batch: &[(UserId, Notification)])` с реализацией по умолчанию (по одному через `notify`). `Notifiers` раздаёт пачку целиком каждому уведомителю.

3. **social-core, доставка: пул воркеров** (`app/fanout.rs`). `AppConfig::delivery_workers` воркеров (по умолчанию 4), у каждого своя очередь `mpsc::channel(queue)`. Получатель выбирается хешем `UserId`, поэтому уведомления одного получателя всегда в одном воркере и в порядке публикации. Воркер копит пачку: отдаёт её `notify_batch`, когда набралось `batch_size` (32) или прошло `batch_window` (50 мс) с первого уведомления пачки. `flush` кладёт метку в очередь каждого воркера и ждёт всех (неполная пачка перед ответом уходит). По отмене токена стадии воркер закрывает очередь, дочитывает её и отдаёт хвост: при остановке ничего не теряется. Воркеры живут под `supervise` с `Restart { restart_pause (100 мс), max_restarts (5) }` и токеном корня: паника уведомителя роняет воркер (пачка, на которой он упал, потеряна), супервизор поднимает его снова на той же очереди (`tokio::sync::Mutex<Receiver>`). `App::restarts() -> u64` считает перезапуски. `catch_unwind` вокруг уведомителя больше нет.

4. **social-core, тренды в акторе** (`app/trends.rs`). Актор (задача) владеет сводкой и принимает запросы `Refresh(oneshot)` и `Top(n, oneshot)` через `mpsc`. `TrendsHandle` (`Clone`): `refresh().await`, `top(n).await` и `latest(n)` (последняя сводка из `watch`, без запроса). `App::trends_handle()`, `App::trends(n)` по-прежнему синхронный (`latest`), `App::refresh_trends()` спрашивает актор под `op_timeout`. Тикер (тик со `Skip`, смена настроек, тишина после `Liked`) стал отдельной задачей-источником: сам он ничего не считает, а просит актор.

5. **social-core, остановка.** `App` держит `Shutdown` со стадиями `sources` (тикер), `handlers` (воркеры доставки) и `trends` (актор). `App::close(deadline).await -> ShutdownReport`, `App::is_closed()`. После `close`: `publish` и `flush` отвечают `AppError::Stopped(Op::Flush)`, `refresh_trends` `Stopped(Op::Trends)`. Если последний `App` ушёл без `close`, `Drop` ядра отменяет корень (запасной путь), и задачи выходят сами. `watch`-сигнал остановки из m18 больше не нужен.

6. **AppConfig** получает `delivery_workers`, `batch_size`, `batch_window`, `restart_pause`, `max_restarts`. Экспорт из корня: `TrendsHandle`.

7. **tools/repl.** `repl::CLOSE_DEADLINE = 5 с`. `Drop` терминала и конец JSON-сеанса делают `close(CLOSE_DEADLINE)`: лог `--log` дописывается целиком, а неполная пачка уведомлений доходит.

8. **Старые тесты.** Веха кладёт новую версию `social-core/tests/journey/m18_async_events.rs`: проверка backpressure теперь на одном воркере с пачкой из одного уведомления (иначе воркер забирает очередь в пачку быстрее, чем она заполнится).

## Дерево после вехи

```
backend/
  Cargo.toml                        + service-runtime в [workspace.dependencies]
  src/crates/
    service-runtime/
      Cargo.toml                    только tokio
      src/
        lib.rs                      только mod и pub use
        token/mod.rs                CancellationToken
        tracker/mod.rs              TaskTracker
        supervisor/mod.rs           supervise, Restart, Outcome
        shutdown/mod.rs             Shutdown, Stage, ShutdownReport, StageReport
      tests/journey/
        m20_clean_shutdown.rs       тесты курса (и строка mod в tests/journey/main.rs)
    social-core/
      Cargo.toml                    + service-runtime
      src/
        lib.rs                      + TrendsHandle
        notify/mod.rs               + notify_batch
        app/
          mod.rs                    Shutdown со стадиями, close, is_closed, restarts
          config.rs                 + пул и супервизор
          fanout.rs                 пул воркеров, пачки, хвост, супервизор
          trends.rs                 актор, TrendsHandle, тикер-источник
      tests/journey/
        m20_clean_shutdown.rs       тесты курса
        m18_async_events.rs         новая версия
  tools/repl/src/
    runtime/mod.rs                  + CLOSE_DEADLINE
    terminal/mod.rs                 close в Drop
    run/mod.rs                      close в конце JSON-сеанса
```

Остальное дерево не меняется.

## Приёмка

- `src/crates/service-runtime/tests/journey/m20_clean_shutdown.rs`: токен виден всем клонам и позднему слушателю, дети следуют за родителем, но не наоборот, учёт дожидается всех после `close`, отменённая задача уходит из учёта, супервизор перезапускает с паузой и сдаётся по лимиту, супервизор останавливается по токену, остановка по стадиям с одним дедлайном и `abort` опоздавших (на замороженном времени);
- `src/crates/social-core/tests/journey/m20_clean_shutdown.rs`: пачки по `batch_size`, неполная пачка по окну, порядок у каждого получателя при четырёх воркерах, хвост доходит при `close`, порядок стадий и отказ после остановки, `abort` воркера, который не успел к дедлайну, перезапуск упавшего воркера, тренды в акторе;
- остальные тесты прошлых вех проходят;
- `cargo make check` зелёный.

Архитектура: крейт `service-runtime` в воркспейсе, без `tokio-util` и без комментариев, токен на `watch`, файлы по дереву; доставка через `supervise` и `notify_batch`, без `catch_unwind`; тренды за `TrendsHandle` с `oneshot`; в ядре `Shutdown` со стадиями; и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
printf 'register alice\nregister bob\nfollow bob alice\npost alice Пока!\n' | cargo run -q -p repl -- --log
```

В stderr лог уведомлений приходит целиком, даже уведомление о посте, отправленном последней строкой: терминал закрывает ядро и ждёт, пока воркер отдаст хвост.
