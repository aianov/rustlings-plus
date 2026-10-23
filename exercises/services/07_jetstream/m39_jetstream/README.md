# Веха 39. Уведомления без потерь

Вчера notify-service перезапустили на минуту, и все лайки за эту минуту пропали: core NATS ничего не хранит, а подписка в группе живёт, только пока жив подписчик. post-service при этом не знает, записал ли брокер его событие, а повтор публикации после таймаута дал бы второе уведомление. Колокольчик, который иногда молчит, хуже, чем колокольчика нет.

Эта веха переносит события соцсети в JetStream. Stream `SOCIAL_EVENTS` ловит темы событий и создаётся при старте, если его нет, без пересоздания. Публикация ждёт подтверждения записи и несёт `Nats-Msg-Id`, поэтому повтор брокер узнаёт. notify-service читает durable pull consumer с постоянным именем: после перезапуска он продолжает с места остановки, подтверждает событие только после записи уведомления, недоступного соседа переживает повторами с растущей паузой, а мусор и безнадёжные события откладывает в мёртвую тему с причиной. Долгую работу он отмечает сигналом «ещё работаю», и два экземпляра делят один durable. Одна дыра остаётся: если брокер недоступен в момент публикации, событие теряется. Её закроет outbox в m40.

Всё собирается из главы 07_jetstream: задачи с svjs1 по svjs13 и проект svjs_quiz (модуль `js` в `nats-engine`). Stream, подтверждение публикации, окно дубликатов, pull consumer, ack, nak с паузой, term, max_deliver, мёртвая тема и progress там уже были, здесь они встают в соцсеть.

## Что сделать

1. **nats-engine**, модуль `js/` (`mod.rs`, `feed.rs`, `dead.rs`):
   - `StreamSpec { name, subjects, duplicate_window, max_age }` (`StreamSpec::new(name, subjects)`: окно дубликатов 120 с, срок жизни без ограничения). `ensure_stream(&bus, &spec)`: stream нет, он создаётся; есть с теми же темами (порядок не важен), не трогается; темы изменились, `update_stream` без потери записей; темы уже ловит чужой stream, это `BusError::Transport`; без связи `Unavailable`. Удаления нет нигде.
   - `publish_event(&bus, &subject, event_id, &body, &meta) -> Result<Published { sequence, duplicate }, BusError>`: без связи сразу `Unavailable`; заголовки `Meta` плюс `Nats-Msg-Id` (`async_nats::header::NATS_MESSAGE_ID`, имя для справки в `MSG_ID`); подтверждение ждётся не дольше `ACK_TIMEOUT` (1 с); на таймауте до `PUBLISH_ATTEMPTS` (3) попыток с тем же идентификатором и паузой 50 мс; если связь пропала посреди повторов, `Unavailable`; любая другая ошибка (тему не ловит ни один stream) сразу `Transport`.
   - `FeedSpec { stream, durable, filter, dead, ack_wait, batch, parallel, max_deliver, backoff }` с `FeedSpec::new(stream, durable, filter, dead)` (5 с, 32, 4, 5 доставок, паузы 100, 200, 400, 800 мс) и `pause(delivered)`. `Verdict { Done, Retry, Poison(причина) }`, `Handler<T>`.
   - `open_feed(&bus, spec, handler) -> Feed`: durable pull consumer через `get_or_create_consumer` (явные подтверждения, `ack_wait`, `max_deliver`, фильтр). Фоновая задача берёт пачки по `batch` с коротким ожиданием и обрабатывает до `parallel` сообщений одновременно. Сообщение, которое не разобралось, сразу уходит в мёртвую тему и получает `Term`, обработчик его не видит. Пока работает обработчик, брокеру раз в треть `ack_wait` идёт `Progress`. `Done` подтверждается. `Retry` возвращается через `Nak` с паузой `pause(delivered)`, а на последней доставке уходит в мёртвую тему и `Term`. `Poison` сразу в мёртвую тему и `Term`. Не удалось записать мёртвое письмо, сообщение возвращается через секунду, а не теряется. `Feed::stop` срабатывает между пачками.
   - Мёртвое письмо: тело как было, заголовки номера запроса и трассы, `X-Original-Subject`, `X-Delivery-Count`, `X-Error` (причина без переводов строки), запись с подтверждением. `Nats-Msg-Id` исходного события в копию не переносится: иначе брокер примет её за дубль и молча не запишет.
   - `fresh_id(prefix)` для идентификаторов событий.

2. **contracts**: в схеме тем `DEAD`, `STREAM_SUFFIX`, `Topics::stream()` (для корня `social` это `SOCIAL_EVENTS`), `stream_subjects()` (`<корень>.events.>` и `<корень>.dead.>`), `dead(consumer)` (`<корень>.dead.<читатель>`).

3. **user-service и post-service**: `BusEvents::open(bus, topics)` готовит stream через `ensure_stream` и только потом отдаёт адаптер; `publish` пишет событие через `publish_event` со свежим `fresh_id("evt")`. Неудача по-прежнему только в лог с номером запроса: подписка и лайк записаны, событие без брокера потеряно (это и чинит m40). Bootstrap открывает адаптер через `BusEvents::open`.

4. **notify-service**:
   - bootstrap готовит тот же stream; `controller/events.rs`: `spec(&config)` (stream из схемы, durable с именем группы `notify-service`, фильтр `<корень>.events.>`, мёртвая тема `<корень>.dead.notify-service`, `ack_wait`, `batch`, `parallel` из `workers`) и `open_feed`. Обработчик: `Ignore` это `Done`, `Reject` это `Poison`, обработанное событие `Done`, `AudienceError::Unavailable` это `Retry` (user-service вернётся), `AudienceError::Failed` это `Poison`.
   - Очереди `mpsc`, `try_send` и подписки в группе больше нет: давление держит сам pull, брокер не отдаёт больше, чем взяли.
   - `Stats { received, ignored, rejected, retried, handled, stored, duplicates }`, `balanced()`: `received = ignored + rejected + retried + handled`.
   - Конфиг: `NOTIFY_ACK_WAIT_MS` (5000), `NOTIFY_BATCH` (32), `NOTIFY_WORKERS` теперь число событий пачки в работе одновременно; `NOTIFY_QUEUE` ушла из `.env.example` и `.env`; нули отвергаются.
   - `Controller::stop`: читатель останавливается между пачками, затем снимаются ответчики. Повторная доставка того же события (подтверждение потерялось) не множит уведомление, пока оно не прочитано: это та же проверка «такое непрочитанное уже есть».

5. **Тесты убирают за собой**: каждый тест работает в своём корне тем, а значит в своём stream, и удаляет его в конце. Для этого `async-nats` в `[dev-dependencies]` шлюза, user-service и post-service, у notify-service ещё `futures`.

## Дерево после вехи

```
backend/src/
  crates/nats-engine/src/js/
    mod.rs                       StreamSpec, ensure_stream, publish_event, Published
    feed.rs                      FeedSpec, Verdict, Handler, open_feed, Feed
    dead.rs                      мёртвое письмо и его заголовки
```

Остальное дерево прежнее.

## Приёмка

Тестам нужен NATS курса с JetStream на `127.0.0.1:54222` (адрес из `RUSTLINGS_NATS_URL`).

- `nats-engine/tests/journey/m39_jetstream.rs`: `ensure_stream` не стирает историю, расширяет темы и отказывает при конфликте; повтор с тем же идентификатором брокер узнаёт, идентификатор лежит в записи; тема без stream это отказ сразу; читатель получает каждое событие один раз и после перезапуска продолжает с места остановки; повтор приходит после растущей паузы; исчерпанные повторы и яд ложатся в мёртвую тему с историей, остальные события проходят; работа втрое дольше `ack_wait` не даёт второй доставки; два читателя с одним именем делят поток.
- `notify-service/tests/journey/m39_jetstream.rs`: перезапуск не теряет и не повторяет; одно событие, опубликованное дважды, это одно уведомление; сбой user-service это повтор с паузой, постоянная ошибка уходит в мёртвую тему с причиной.
- `user-service/tests/journey/m39_jetstream.rs`: подписка лежит в stream со своим идентификатором, повторный запуск stream не трогает.
- `api-gateway/tests/journey/m39_jetstream.rs`: пока notify-service лежит, пушей нет; после его запуска приходят подписка и лайк, колокольчик знает оба.
- новые версии тестов прошлых вех: m38 notify-service (мусор и лайк без поста в мёртвой теме, группа это общий durable, тест очереди с потерями ушёл), m38 user-service и post-service (`BusEvents::open`), `support.rs` шлюза удаляет stream при остановке.
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву; в коде сервисов и в `nats-engine` нет `delete_stream`; stream создаётся через `get_or_create_stream` и расширяется через `update_stream`; публикация несёт `NATS_MESSAGE_ID`; в читателе `AckPolicy::Explicit`, `max_deliver`, `Progress`, `Nak` с паузой и `Term`; notify-service читает `open_feed`, а `subscribe_group` в его контроллере нет; и все договорённости прошлых вех.

## Как запустить

NATS курса должен работать: `127.0.0.1:54222`, тот же, что в главах 06_nats и 07_jetstream.

```
cd backend
cargo make check
cargo make demo
```

В клиенте alice: `register alice correct-horse`, `login alice correct-horse`, `post привет`. Теперь останови notify-service (во втором окне `pkill -f target/debug/notify-service`) и во втором клиенте (`cargo run -p social-cli`) от имени bob сделай `follow alice` и `like 1`: пушей у alice нет, `bell` говорит, что уведомления недоступны. Запусти notify-service снова (`cargo run -p notify-service`): alice тут же получит «на тебя подписался» и «лайкнул твой пост [1]», а `bell` покажет оба. События ждали в stream `SOCIAL_EVENTS`, а durable `notify-service` помнил, где остановился.
