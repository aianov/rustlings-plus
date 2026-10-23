# Веха 38. События через брокер

Пуши соцсети до сих пор делает шлюз. После `users.follow` он сам собирает `Push::Followed`, после `posts.like` собирает `Push::Liked`, а после `posts.create` спрашивает у user-service, кто читает автора, и рассылает `Push::NewPost`. Шлюз знает слишком много: он решает, кого и о чём уведомить, офлайн-пользователь теряет всё, списка непрочитанного нет, а лайк больше никому не виден, хотя завтра он понадобится трендам и ленте. Так дальше нельзя.

В этой вехе сервисы начинают говорить фактами. user-service объявляет в NATS «на alice подписались», post-service объявляет «пост создан, лайкнут, прокомментирован». Новый `notify-service` слушает эти события в группе, копит уведомления и отвечает на вопрос «сколько непрочитанных». Тяжёлый веер нового поста он отдаёт ограниченной очереди с работниками. Шлюз только подписан на темы уведомлений и пушит то, что пришло. Если брокер пропал, подписка и лайк всё равно работают, а событие теряется: это честно сказано в логе, а чинить будем в m39 JetStream'ом. notify-service при этом снимает готовность.

Всё собирается из главы 06_nats: задачи с svnats1 по svnats16, проект svnats_engine (крейт `nats-engine`) и проект svnats_quiz (сервис уведомлений). Тема, шаблон, круг до брокера, группа, запрос с исходами, медленный потребитель, ограниченная очередь там уже были, здесь они встают в соцсеть.

## Что сделать

1. **nats-engine** (`src/crates/nats-engine`): крейт из проекта svnats_engine, разложенный по папкам: `subject/` (`token`, `Subject`, `Pattern`, `Seg`, `SubjectError`, `MAX_TOKEN`), `meta/` (`Meta`, `MetaError`, `good_id`, заголовки `X-Request-Id` и `X-Trace-Id`), `config/`, `error/` (`BusError`), `bus/` (`Bus`, `Envelope`, в `subscription.rs` подписка, в `server.rs` ответчик). Отличия от проекта:
   - `Bus` клонируется, `BusConfig { url, name, subscription_capacity, connect_timeout }` с `BusConfig::new(url).named(имя)`; `NATS_URL_ENV = "NATS_URL"`, `DEFAULT_NATS_URL = "nats://127.0.0.1:54222"`. Пауза переподключения 50 мс на попытку, но не больше 200 мс.
   - `Meta::follow(request_id, trace)`: годные номера идут в заголовки как есть, негодный номер запроса заменяется свежим, без трассы её начинает сам номер запроса.
   - `serve` принимает асинхронный обработчик `Fn(Envelope<Q>) -> impl Future<Output = Result<A, String>>`: хранилище сервиса живёт за трейтом с `BoxFuture`. Ответ на проводе `{"ok": значение}` или `{"err": "текст"}`.
   - `subscribe`, `subscribe_group` и `serve` без связи сразу отвечают `Unavailable`.
   - Зависимости только `async-nats =0.36.0`, `tokio`, `serde`, `serde_json`, `bytes`, `futures` (в `[workspace.dependencies]` корня, как всё остальное).

2. **contracts**.
   - `topics/` (схема тем соцсети): `ROOT_ENV = "EVENTS_ROOT"`, `DEFAULT_ROOT = "social"`, `NOTIFY_GROUP = "notify-service"`. `EventKind { UserFollowed, PostCreated, PostLiked, PostCommented }` с `entity`, `action`, `of`, `ALL`. `EventBody { actor, post }` (поста у подписки нет, и он не пишется), `Event { kind, owner, body }` с `followed(actor, target)`, `post_created(author, post)`, `post_liked(actor, author, post)`, `post_commented(actor, author, post)`: владелец события это тот, о ком факт (цель подписки или автор поста). `Origin { request_id, trace_id }` (`new`, `traced`). `NotifyMethod { Unread, List, Read }`.
   - `Topics::new(root)`: `event` даёт `<корень>.events.<сущность>.<действие>.<владелец>`, `all_events` даёт `<корень>.events.>`, `read_event(тема)` разбирает обратно в вид и владельца (чужой корень, лишнее слово или неканонический id вроде `u_02` это `None`), `notice(user)` даёт `<корень>.notices.<user>`, `all_notices` даёт `<корень>.notices.*`, `read_notice`, `notify(method)` даёт `<корень>.notify.<метод>`. Слова тем это `Vec<String>`: проверяет их `nats-engine`.
   - `notice/`: `NoticeKind { Followed, Liked, Commented, NewPost }` (`snake_case` на проводе), `NoticeView { id, to, kind, actor, post, read }` с `push()` в прежний `Push` (у нового поста актор это автор; уведомление о посте без поста не пушится), `UnreadView { unread }`, `NoticesView { notices, unread }`, `MarkedView { marked }`, запрос `NoticeAsk { user, limit }`, `DEFAULT_NOTICES = 20`, `MAX_NOTICES = 100`.
   - `Push::Commented { to, by, post }`, `CommentView { id, post }`. Методы `posts.comment { post, text }`, `notify.unread {}`, `notify.list { limit }`, `notify.read {}`: в `METHOD_NAMES` их 16 (`posts.comment` после `posts.feed`, три `notify.*` в конце), варианты `Call::{Comment, Unread, Notices, ReadNotices}`, `Reply::{Comment, Unread, Notices, Marked}`. Разбор вызовов переезжает в `api/decode.rs`: `api/mod.rs` не длиннее 300 строк.

3. **grpc-kit**: `Stamp::trace_id()` отдаёт пропущенный клиентский `x-trace-id`.

4. **user-service**: порт `service/events.rs` (`trait Events { fn publish(&self, event: Event, origin: Origin) -> BoxFuture<'_, ()> }` и тихая реализация `Silent`), `UserService::with_events`. `follow(who, whom, &origin)` публикует `Event::followed` только если подписка случилась: повтор, подписка на себя и на несуществующего событий не дают. Адаптер `client/events.rs`: `BusEvents::new(bus, topics)` собирает тему из схемы и `Meta::follow` из `Origin`; неудачная публикация пишется в лог с номером запроса, а ответ на `Follow` всё равно успешный. Контроллер строит `Origin` из `Stamp::follow`. Конфиг: `nats: Option<BusConfig>` из `NATS_URL` (без переменной сервис работает с `Silent`), `topics` из `EVENTS_ROOT` (корень проверяется как слово темы); `bootstrap::connect` подключает шину до старта, а после остановки `close` дописывает её буфер; `Error::Bus`.

5. **post-service**: так же. `create` публикует `post.created`, `like` с изменением публикует `post.liked` (свой лайк тоже: событие это факт, а решать, уведомлять ли, будет потребитель), `comment` публикует `post.commented` с владельцем, равным автору поста (пост читается до записи комментария). Отказ событий не даёт.

6. **notify-service** (новый сервис по тем же слоям):
   - `common/`: `Config` (`NOTIFY_ADMIN_ADDR` 127.0.0.1:9885, `NATS_URL` по умолчанию `DEFAULT_NATS_URL`, `EVENTS_ROOT`, `USERS_GRPC_ADDR`, `NOTIFY_WORKERS` 4, `NOTIFY_QUEUE` 256, `NOTIFY_AUDIENCE_TIMEOUT_MS` 2000, `CERTS_DIR`; нули и кривой корень отвергаются), `Error`, `Incoming`, `Draft`, `Notice` (`view`, `same`), `Stats` и `StatsSnapshot` с `balanced()`.
   - `dto/`: `decode(&topics, &envelope) -> Decoded { Notify(Incoming), Ignore, Reject(причина) }`. Незнакомая тема или действие это `Ignore`, событие о посте без `post` это `Reject`, `post.created` от не того автора это `Reject`, действие над собой (актор равен владельцу, кроме создания поста) это `Ignore`, у подписки пост отбрасывается. `limit(&ask)` проверяет границы списка.
   - `service/`: порты `Audience` (кто читает автора, `AudienceError { Unavailable, Failed }`) и `Courier` (отправить уведомление дальше, `Quiet` для тестов). `NotifyService::new(notices, audience, courier)`, `handle(incoming) -> Result<Handled { stored, duplicates }, AudienceError>`: подписка, лайк и комментарий дают одно уведомление владельцу; новый пост даёт `NewPost` каждому читателю из `Audience`, кроме автора. Каждое уведомление записывается, сохранённое уходит в `Courier`. `unread`, `list` (от новых к старым, не больше limit), `mark_read`.
   - `repository/`: `NoticeRepository { record, unread, list, mark_read }` и `MemoryNotices`. `record` не заводит уведомление, если у адресата уже есть такое же непрочитанное (вид, актор, пост): проверка и вставка под одним замком, ведь работников несколько. Прочитанное не мешает новому такому же.
   - `client/`: `GrpcAudience` (`Relations` user-service, поле `audience`, со сроком; `Unavailable`, `DeadlineExceeded` и `Cancelled` это `AudienceError::Unavailable`; номер запроса события едет в `x-request-id`) и `BusCourier` (публикует `NoticeView` в `<корень>.notices.<адресат>` с номером запроса события).
   - `controller/events.rs`: подписка в группу `notify-service` на `<корень>.events.>`, ограниченная очередь `mpsc` на `queue` мест и ровно `workers` работников с общим приёмником. Приём ничего тяжёлого не делает и не ждёт: полная очередь это `dropped`. Каждое полученное сообщение попадает ровно в один из счётчиков `ignored`, `rejected` (и сломанный JSON тоже), `queued`, `dropped`, поэтому `received = ignored + rejected + queued + dropped`; работники считают `stored`, `duplicates` и `failed`. `controller/requests.rs`: ответчики `unread`, `list` и `read` в той же группе. `Controller::stop`: закрыть приём (отписка и круг до брокера, то, что уже в буфере, ещё доходит до очереди), снять ответчиков, дождаться работников.
   - `bootstrap/`: `start(admin, config, shutdown) -> Running` возвращается, когда подписки уже действуют (`stats()`, `ready()`, `finished()`), `run` и `serve`; проверка готовности `nats` это состояние связи. `main.rs` как у соседей. Тестам нужны `async-nats` (сырой мусор в шину) и `user-service` (настоящий веер) в `[dev-dependencies]`. Пока хранилище в памяти, второй экземпляр отвечает только про свои уведомления: группа уже гарантирует, что события не задвоятся, а общее хранилище придёт вместе с базой.

7. **certgen**: в `SERVICES` появляется `notify-service`: в user-service он ходит по взаимному TLS со своим сертификатом.

8. **api-gateway**: самодельных пушей больше нет. `users.follow`, `posts.like` и `posts.create` пушей не собирают, `Relations` шлюз больше не зовёт. Конфиг: `nats: Option<BusConfig>` (`NATS_URL`), `topics` (`EVENTS_ROOT`), `notify_timeout` (`GATEWAY_NOTIFY_TIMEOUT_MS`, 1000, ноль отвергается).
   - `bootstrap`: шина подключается до старта, подписка на `<корень>.notices.*` действует до первого соединения, после остановки буфер дописывается.
   - `controller/notices.rs`: уведомление становится `Push` и уходит во все соединения адресата, если пользователь из темы совпадает с `to`; иначе оно выброшено с записью в лог.
   - `client/notify.rs`: `Notify::ask(method, &ask, &meta)`. `Unavailable` и `NoService` это `unavailable` («уведомления сейчас недоступны»), `Timeout` это `timeout`, остальное `internal` с записью в лог. `Ctx::with_notify` и `ctx.notify(method, &ask)`: без NATS ответ `unavailable` «уведомления выключены».
   - `routes/notify.rs`: `notify.unread`, `notify.list` (`limit` от 1 до 100, иначе `bad_data`), `notify.read`; все защищённые, пользователь берётся из `Principal`, а не из `data`. `posts.comment { post, text }` отвечает `CommentView`.
   - Сообщения чата по-прежнему идут потоком chat-service: темы пользователя появятся в m41.
   - Тестовый стек поднимает notify-service, поэтому он в `[dev-dependencies]` шлюза.

9. **Корень воркспейса**: `async-nats = "=0.36.0"`, `futures = "0.3"`, пути `nats-engine` и `notify-service` в `[workspace.dependencies]`. В `.env.example` и `.env`: `GATEWAY_NOTIFY_TIMEOUT_MS`, `NOTIFY_ADMIN_ADDR`, `NATS_URL`, `EVENTS_ROOT`, `NOTIFY_WORKERS`, `NOTIFY_QUEUE`, `NOTIFY_AUDIENCE_TIMEOUT_MS`. В `Makefile.toml` задача `notify-service`, она же в `run-all` и `demo`.

10. **tools/social-cli** приходит новой версией: `comment <id> <текст>`, `bell` (непрочитанных столько-то и последние уведомления, непрочитанные с точкой), `read` (всё прочитано) и push «прокомментировал твой пост». Незнакомый ник из пуша или колокольчика клиент сам узнаёт через открытый `users.profile`, поэтому вместо `u_2` видно `@bob`.

## Дерево после вехи

```
backend/src/
  crates/nats-engine/
    src/bus/                       Bus, подписка (subscription.rs), ответчик (server.rs)
    src/config/  src/error/        BusConfig, BusError
    src/meta/  src/subject/        заголовки X-Request-Id и X-Trace-Id, темы и шаблоны
  crates/contracts/src/
    topics/                        схема тем: события, уведомления, запросы к notify-service
    notice/                        виды уведомлений и запросов колокольчика
    api/decode.rs                  разбор вызовов
  microservices/user-service/src/
    service/events.rs  client/events.rs
  microservices/post-service/src/
    service/events.rs  client/events.rs
  microservices/notify-service/
    src/main.rs  src/lib.rs
    src/bootstrap/                 start, run, serve, готовность по связи с NATS
    src/controller/                events.rs (группа, очередь, работники), requests.rs
    src/dto/                       событие в Incoming, границы списка
    src/service/                   NotifyService, порты audience.rs и courier.rs
    src/repository/                NoticeRepository, memory.rs
    src/client/                    users.rs (Relations), courier.rs (публикация уведомлений)
    src/common/                    config.rs, error.rs, notice.rs, stats.rs
  microservices/api-gateway/src/
    client/notify.rs  controller/notices.rs  routes/notify.rs
tools/social-cli/                  comment, bell, read (новая версия)
```

## Приёмка

Тестам нужен NATS курса на `127.0.0.1:54222` (адрес из `RUSTLINGS_NATS_URL`); каждый тест берёт свой корень тем.

- `nats-engine/tests/journey/m38_nats_events.rs`: слова темы проверяются, подстановку даёт только `Seg`; `Meta::follow` берёт годное и заменяет негодное; событие едет ровно с двумя заголовками; сломанное сообщение это одна ошибка, поток живёт дальше; запрос различает `NoService`, `Remote`, `Timeout`, асинхронный обработчик отвечает, остановленный ответчик снят до возврата; без связи 3000 публикаций отказывают сразу, готовность падает и возвращается сама.
- `notify-service/tests/journey/m38_nats_events.rs`: разбор событий (кому, игнор, мусор); одинаковое непрочитанное сливается, прочитанное не мешает, список от новых; 32 гонщика сохраняют одно уведомление; веер нового поста идёт читателям, но не автору, сломанный `Audience` это ошибка; запросы `unread`, `list`, `read` и отказ на кривой limit; сломанные и чужие события посчитаны, баланс сходится; полная очередь выбрасывает и считает, остановка доделывает очередь; два экземпляра в группе делят поток без дублей; готовность и `/ready` следуют за брокером; веер через настоящий user-service учитывает скрытых авторов и несёт номер запроса.
- `user-service` и `post-service` (`m38_nats_events.rs`): событие только после настоящего изменения, с номером запроса; адаптер кладёт факт в его тему; подписка проходит, когда брокер пропал; `NATS_URL` и `EVENTS_ROOT` читаются из окружения, кривой корень отвергается.
- `api-gateway/tests/journey/m38_nats_events.rs`: подписка звонит в колокольчик цели, `unread`, `list`, `read` работают и у каждого свои; лайк и комментарий доходят до автора, свои действия молчат; новый пост доходит до подписчиков через notify-service и не доходит до чужих; без notify-service шлюз сам ничего не пушит, а колокольчик отвечает `unavailable`; уведомление из шины доходит только до своего адресата; номер WS-запроса становится заголовком события; кривой limit, пустой комментарий и гость получают свои отказы; клиент курса показывает пуши, `bell`, `read` и `comment`.
- `contracts/tests/journey/m38_nats_events.rs`: схема тем, чужие и кривые темы, тело события, уведомление в прежний push, новые методы контракта.
- новые версии тестов прошлых вех: `support.rs` шлюза поднимает notify-service и подключает сервисы к NATS со своим корнем; пуши в m24 и m30 сверяются без порядка (лайк и подписка идут событием, сообщение потоком chat-service); m30 больше не ждёт пуша о посте от шлюза; m33, m36 и m37 сервисов передают `Origin`; m13 contracts знает 16 методов.
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву; `nats-engine` стоит только на async-nats, tokio, serde, serde_json, bytes и futures; в коде сервисов нет `async_nats::`, а в их `service/` нет `nats_engine::`; в обработчиках маршрутов шлюза нет `Push::`; группа, `try_send` и ограниченная очередь в приёме notify-service, `register("nats"` в его bootstrap; и все договорённости прошлых вех.

## Как запустить

NATS курса должен работать: тот же брокер, что в главе 06_nats, `127.0.0.1:54222`.

```
cd backend
cargo make check
cargo make demo
```

В первом окне клиента: `register alice correct-horse`, `login alice correct-horse`, `post привет`. Во втором (`cargo run -p social-cli`): `register bob correct-horse`, `login bob correct-horse`, `find alice`, `follow alice`, `like 1`, `comment 1 согласен`. В окне alice по очереди появятся «на тебя подписался @bob», «@bob лайкнул твой пост [1]» и «@bob прокомментировал твой пост [1]», а `bell` покажет три непрочитанных; `read` отметит их прочитанными. Если остановить NATS (`docker stop rustlings-nats`), `follow` и `like` продолжают работать, в логе сервиса появится «событие … не ушло», `curl http://127.0.0.1:9885/ready` ответит 503, а `bell` скажет, что уведомления недоступны. После `docker start rustlings-nats` всё вернётся без перезапуска сервисов.
