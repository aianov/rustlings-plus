# Веха 40. Лента из событий

После m39 одна дыра осталась открытой: user-service сначала пишет подписку в журнал, а потом публикует событие. Если брокер в этот момент недоступен, подписка есть, а события нет, и notify-service о ней никогда не узнает. Вторая беда в ленте: на каждый показ post-service собирает её сам и ходит в user-service за подписками и именами. Чем больше людей листает ленту, тем больше нагрузка ложится на соседей, которым до неё нет дела.

Эта веха наводит в событиях порядок. В contracts появляется конверт доменного события с типом, схемой, версией агрегата и временем, его читатель терпит лишние поля и поднимает старую схему до новой. user-service и post-service больше не публикуют сами: изменение и его событие ложатся в журнал одной строкой (outbox), а релей в `client/` отправляет их в JetStream с `Nats-Msg-Id = event_id` и только потом помечает отправленное. Новый сервис feed-service держит свою проекцию: авторов, подписки, глушения и посты. Он применяет события строго по версии агрегата, повтор пропускает, а при старте строит проекцию заново с начала stream. Шлюз берёт ленту у него.

Всё собирается из главы 08_event_patterns: задачи с svevt1 по svevt15 и проект svevt_quiz. Конверт, терпимый читатель, апкаст, идемпотентность, inbox, outbox, релей, порядок по версии, проекция и пересборка там уже были, здесь они встают в соцсеть с настоящим JetStream.

## Что сделать

1. **contracts**, модуль `domain/` (`mod.rs`, `upcast.rs`):
   - `DomainEvent { event_id, kind, schema, version, occurred_at, aggregate_id, payload }`. В JSON поле `kind` называется `type`. Читатель терпимый: лишние поля пропускаются, нет `schema`, значит 1, нет `payload`, значит `null`. `DomainEvent::new(event_id, &fact, version, occurred_at)` берёт тип, схему, агрегат и данные из факта. `entity()` и `action()` это две половины типа (`post` и `liked`).
   - Факты: `Registered { user_id, username }`, `ProfileChanged { user_id, display_name }` (схема 2, `display_name` может быть `null`), `Followed { follower_id, followee_id }`, `Muted { user_id, muted_id }`, `PostCreated { post_id, author_id, author_name, text, tags, links }`, `PostLiked { post_id, author_id, user_id }`, `PostCommented { post_id, author_id, comment_id, commenter_id }`, `PostEdited { post_id, text }`, `PostDeleted { post_id }`. `enum Fact` с `kind()`, `schema()`, `aggregate()`, `payload()` и `Fact::parse(kind, &payload)`. Агрегат пользовательских фактов это пользователь (у подписки подписчик), постовых это пост. Типы перечислены в `KINDS` (девять штук), новейшая схема профиля в `PROFILE_SCHEMA` (2).
   - `DomainEvent::fact() -> Result<Option<Fact>, FactError>`: незнакомый тип это `Ok(None)`, а не ошибка; знакомый сначала проходит `upcast`, потом разбирается. `FactError { UnknownSchema { kind, schema }, BadPayload { kind, reason } }`.
   - `upcast`: `user.profile_changed` схемы 1 (`{"name": "..."}`) становится схемой 2: имя без пробелов по краям в `display_name`, пустое имя это `null`, `user_id` берётся из `aggregate_id`, чужие поля данных остаются. Нет `name` или это не строка, `BadPayload`. Схема, которую код не знает (например, 3), это `UnknownSchema`. Остальные типы не меняются.
   - В `match` по строкам нет `_ =>`: незнакомое значение называется по имени (`_unknown =>`).

2. **contracts**, модуль `outbox/`:
   - `Cause { origin, at }` с `Cause::new(&origin, at)`: откуда пришёл запрос и когда (микросекунды). `OutboxRow { seq, event, request_id, trace_id }`.
   - `Outbox`: `prepare(&fact, &cause)` готовит строку со следующим сквозным `seq`, следующей версией агрегата и свежим `fresh_event_id()` (`evt-` и 32 шестнадцатеричных знака), но ничего не меняет; `push(row)` принимает строку (и при записи, и при чтении журнала после перезапуска), `published_through(seq)` снимает отправленное, `unpublished(limit)` отдаёт ждущие по порядку `seq`, `published()`, `pending()`, `version(aggregate)`.
   - Схема тем: событие `<корень>.events.<entity>.<action>.<aggregate_id>`, `Topics::event(&event)`. Старые `Event`, `EventKind`, `EventBody` и `read_event` уходят.

3. **nats-engine**, `js/replay.rs`: `replay::<T>(&bus, stream, &filter) -> Replay<T>` открывает упорядоченного читателя с первой записи stream и знает, сколько записей было в stream на старте (`pending_at_start()`); `next()` отдаёт `Replayed { sequence, pending, item }`, где `item` это разобранный конверт или ошибка разбора. `last_sequence(&bus, stream)`.

4. **user-service и post-service**:
   - Строка журнала это `UserLine` (у постов `PostLine`): само изменение (`#[serde(flatten)]`) и `outbox: Option<OutboxRow>`. Изменение, которое рождает событие, пишется одной строкой с ним: либо в журнале оба, либо ни одного. Отметка отправленного это тоже строка журнала, `Published { through }`. После перезапуска журнал восстанавливает и данные, и outbox, и отправленное уже не уходит второй раз.
   - Методы записи репозитория принимают `&Cause`, сервис строит его из `Origin` запроса и часов (`common/clock.rs`, `now_micros()`). Порт `Events` и адаптер `client/events.rs` удаляются: сервис о шине не знает совсем.
   - Трейт `OutboxRepository`: `unpublished(limit)`, `mark_published(through)`, `waiting()`, `signal()` (сигнал о новой строке).
   - `client/relay.rs`: `Relay::open(bus, topics, outbox)` готовит stream через `ensure_stream`. `pump()` берёт до `RELAY_BATCH` (64) строк, публикует их по порядку через `publish_event` с `event_id` из строки и заголовками запроса из неё же, на первой ошибке останавливается и помечает отправленное до последней удачной. `run()` крутит `pump`: пусто, ждёт сигнала не дольше секунды; ошибка, пишет в лог один раз за полосу неудач и повторяет через 500 мс. Если релей упал между публикацией и отметкой, после перезапуска он отправит ту же строку с тем же `event_id`, и брокер её узнает.
   - bootstrap запускает релей фоновой задачей «релей outbox» (необязательная, перезапуск через секунду). Запись не ждёт брокера: без NATS подписки, лайки и посты пишутся, события копятся в outbox и уходят, когда брокер вернётся.

5. **notify-service** читает `DomainEvent` вместо старого `Event`: `Trigger { Followed, Liked, Commented, Posted }`, свои действия (лайк своего поста, свой комментарий) по-прежнему тишина, сломанный факт уходит в мёртвую тему с причиной.

6. **feed-service** (новый, `src/microservices/feed-service`, слои как у остальных):
   - `proto/social/v1/feed.proto`: `service Feeds { rpc Page(FeedRequest) returns (FeedReply); }`, адрес `FEED_GRPC_ADDR` (127.0.0.1:50055), admin `FEED_ADMIN_ADDR` (127.0.0.1:9886), свой сертификат из certgen, отвечает только шлюзу (`Gate::only`).
   - `common/change.rs`: `Change` (что событие меняет в проекции, `Nothing` для незнакомого и для комментариев), `Consumed { Applied, Buffered, Duplicate }`, `FeedItem`.
   - `repository/`: `FeedRepository { apply(aggregate, version, change), page(viewer, skip, take), snapshot() }`, `MemoryFeed`. `apply` это inbox: под одной блокировкой проверяется версия агрегата, применяется эффект и сдвигается версия. Версия меньше ожидаемой, `Duplicate`; больше, событие ждёт своей очереди (`Buffered`, повтор ждущего тоже `Duplicate`); ровно ожидаемая, `Applied`, и следом применяются все дождавшиеся.
   - Видимость: свои посты и посты тех, на кого подписан и кого не заглушил; удалённые не видны. Имя автора: `display_name`, иначе `username`, иначе имя из события поста. Порядок: время события по убыванию, при равенстве больший номер поста выше. Время поста берётся из `occurred_at` события, а не из часов: пересборка даёт ту же ленту.
   - `service/`: `consume(&event)` превращает факт в `Change` и отдаёт в репозиторий. Незнакомый тип применяется как `Nothing` и тоже сдвигает версию, иначе следующее событие агрегата ждало бы вечно. Сломанный факт тоже сдвигает версию без эффекта и возвращается ошибкой (в лог). `page(viewer, page, size)`.
   - `controller/events.rs`: `follow(bus, topics, service, progress)` читает stream через `replay` с начала по фильтру `<корень>.events.>`. `Progress` знает, до какой записи применено и догнал ли читатель stream. `controller/grpc.rs`: `FeedsController::page` берёт зрителя из заголовков шлюза, `dto::page` проверяет страницу (с 1) и размер (1..=100).
   - bootstrap: stream через `ensure_stream`, проверка готовности `projection` (не готов, пока не догнал stream), фоновая задача «проекция ленты».

7. **api-gateway**: `GET /feed` идёт в feed-service (`ctx.feed().page(..)`), ответ прежний. Нет feed-service, 503 «сервис ленты недоступен, повтори позже», остальные маршруты работают. Готовность шлюза знает feed-service.

8. **Окружение**: feed-service в воркспейсе, в `.env.example` и `.env`, в задачах `run-all` и `demo` в `Makefile.toml`.

## Дерево после вехи

```
backend/src/
  crates/contracts/src/
    domain/
      mod.rs                     DomainEvent, факты, Fact, FactError, KINDS
      upcast.rs                  profile_changed схемы 1 в схему 2
    outbox/mod.rs                Cause, OutboxRow, Outbox, fresh_event_id
  crates/nats-engine/src/js/
    replay.rs                    упорядоченное чтение stream с начала
  crates/proto/proto/social/v1/
    feed.proto                   сервис Feeds
  microservices/user-service/src/
    common/clock.rs              now_micros
    client/relay.rs              релей outbox
    repository/
      entry.rs                   строка журнала: изменение и его событие
      outbox.rs                  OutboxRepository для MemoryUsers
  microservices/post-service/src/
    client/relay.rs              релей outbox
    repository/
      state.rs                   состояние постов
      outbox.rs                  OutboxRepository для MemoryPosts
  microservices/feed-service/
    Cargo.toml
    src/
      main.rs
      lib.rs
      bootstrap/mod.rs
      common/{mod.rs, change.rs, config.rs, error.rs}
      controller/{mod.rs, events.rs, grpc.rs}
      dto/mod.rs
      repository/{mod.rs, memory.rs}
      service/mod.rs
```

`client/events.rs` и `service/events.rs` в user-service и post-service удалены. Остальное дерево прежнее.

## Приёмка

Тестам нужен NATS курса с JetStream на `127.0.0.1:54222` (адрес из `RUSTLINGS_NATS_URL`).

- `contracts/tests/journey/m40_outbox_feed.rs`: конверт на проводе ровно такой, как описан; читатель терпит лишние поля и отсутствие схемы, незнакомый тип не ошибка; профиль схемы 1 поднимается до 2, неизвестная схема отвергается; каждый факт проходит конверт туда и обратно и называет свой агрегат; outbox нумерует строки сквозь и версии по агрегату.
- `user-service/tests/journey/m40_outbox_feed.rs`: изменение и событие это одна строка журнала; релей публикует по порядку, а падение до отметки не даёт дубля в stream; без брокера события ждут в outbox и уходят, когда он появился.
- `post-service/tests/journey/m40_outbox_feed.rs`: правка и удаление поста это факты в той же строке журнала.
- `feed-service/tests/journey/m40_outbox_feed.rs`: лента собирается из событий; любой порядок и повторы дают ту же проекцию; событие из будущего ждёт, повтор пропускается; новое имя видно и в старых постах, даже из события схемы 1; глушение, правка и удаление меняют показ; незнакомое событие сдвигает версию; одно событие, доставленное восемь раз одновременно, считается один раз; перезапуск строит ту же ленту из stream.
- `api-gateway/tests/journey/m40_outbox_feed.rs`: лента приходит из feed-service и следует за переименованием через `PATCH /users/me`; перезапущенный feed-service собирает ленту заново.
- новые версии тестов прошлых вех: m24, m30 и m32 шлюза (лента догоняет события, падение соседа проверяется на создании поста, без feed-service нет ленты), m29 (медленный телефон заваливают запросами с шагом, чтобы тяжёлых ответов гарантированно было больше буферов сокета; два окна alice ждут подписку и сообщение в любом порядке, потому что событие подписки теперь идёт через outbox), `support.rs` (stream удаляется, пока брокер не подтвердит, что его нет); клиент курса после докачки аватара делает `flush`: `tokio::fs::File` пишет в фоне, и без него тест m27 иногда читал недописанный файл; m31, m34 и m37 (feed-service в стенде и в готовности), m38 и m39 (события в новом конверте, user-service и post-service пишут через outbox), m33, m36 и m37 сервисов (методы записи с `&Origin` и `&Cause`).
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву; feed-service в воркспейсе и с теми же слоями; в `service/` ни одного сервиса нет `nats_engine::`; строка журнала несёт `outbox: Option<OutboxRow>` и отметку `Published`; релей публикует `row.event.event_id`, ждёт сигнала и вызывает `mark_published`; feed-service читает через `replay`, буферизует события из будущего, сдвигает версию на незнакомом и берёт время из события; шлюз зовёт `ctx.feed()`; и все договорённости прошлых вех.

## Как запустить

NATS курса должен работать: `127.0.0.1:54222`, тот же, что в главах 06_nats, 07_jetstream и 08_event_patterns.

```
cd backend
cargo make check
cargo make demo
```

В клиенте alice: `register alice correct-horse`, `login alice correct-horse`, `post привет`, `post как дела`. Во втором клиенте (`cargo run -p social-cli`) bob: `register bob correct-horse`, `login bob correct-horse`, `follow alice`, `feed`: два поста alice, лента пришла из feed-service. Теперь alice делает `set name Алиса`, а bob снова `feed`: у обоих старых постов автор уже «Алиса», хотя посты писались до переименования. Перезапусти feed-service (`pkill -f target/debug/feed-service`, затем `cargo run -p feed-service`): он прочитает stream `SOCIAL_EVENTS` с начала, и лента bob будет той же.
