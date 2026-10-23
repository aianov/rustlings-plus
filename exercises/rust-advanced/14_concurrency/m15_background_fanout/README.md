# Веха 15. Фоновая рассылка

У alice появились подписчики, и они хотят знать, когда она пишет. Сделать это в лоб легко: опубликовал пост, обошёл подписчиков, каждому положил уведомление. Только у популярного автора подписчиков тысячи, а уведомители бывают медленными (лог в файл, завтра почта и пуши в телефон). Если рассылать прямо в `publish`, автор ждёт, пока уйдёт последнее письмо, а вся сеть стоит за замком: ни зарегистрироваться, ни открыть ленту.

Поэтому рассылка уходит в фон, в свой пул потоков. `publish` только ставит задания и сразу отвечает, а воркеры разносят уведомления параллельно и без замка сети. Заодно сеть научится считать, сколько всего сделано (атомиками, без общего замка), и держать кэш профилей, который читается, даже когда сеть занята. Всё на `std`, потоки настоящие, и каждая гонка тут тоже настоящая.

## Что сделать

1. **Новый крейт `src/crates/worker-pool`: свой пул потоков.** Пакет `worker-pool` (библиотека `worker_pool`), общие поля и `[lints]` из воркспейса, зависимостей нет совсем, только `std`. Комментариев в крейте нет. Корень объявляет его в `[workspace.dependencies]`: `worker-pool = { path = "src/crates/worker-pool" }`.
   - `ThreadPool::new(workers: usize) -> io::Result<ThreadPool>` запускает `workers` именованных потоков через `thread::Builder` (ошибку ОС отдаёт, а не паникует); `new(0)` запускает один. `workers() -> usize` отвечает, сколько их.
   - `execute(&self, job: impl FnOnce() + Send + 'static) -> Result<(), PoolClosed>` кладёт задание в очередь. Очередь, число работающих заданий и флаг закрытия живут под одним `Mutex`, воркеры ждут работу на `Condvar`. Задание выполняется после того, как воркер отпустил замок очереди, иначе пул из четырёх потоков работает как один.
   - `submit<T: Send + 'static>(&self, job: impl FnOnce() -> T + Send + 'static) -> Result<JobHandle<T>, PoolClosed>`: результат приходит через канал, `JobHandle::join(self) -> Result<T, JobError>`.
   - Паника в задании не убивает воркер (`catch_unwind`): `join` такого задания отдаёт `JobError::Panicked`, а воркер берёт следующее.
   - `wait_idle(&self)` ждёт на втором `Condvar`, пока очередь пуста и ни одно задание не работает. `pending(&self) -> usize` это число заданий, которые ещё ждут в очереди.
   - `close(&self)`: новые задания получают `Err(PoolClosed)`, уже поставленные доделываются, воркеры выходят, когда очередь опустела.
   - `Drop` закрывает пул и ждёт (`join`) все воркеры. Никаких `sleep` и `yield_now` в коде пула: ожидание только на `Condvar`.
   - `PoolClosed` (unit-структура) и `JobError { Panicked }`: `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Display` и `std::error::Error`.

   Это тот самый пул из проекта `conc_thread_pool`; можно перенести свой и подогнать под этот API.

2. **contracts.**
   - `Push::NewPost { to: UserId, author: UserId, post: PostId }`, на проводе `{"push": "new_post", "to": "u_2", "author": "u_1", "post": "p_7"}`.
   - `Command::Stats` из строки `stats` без аргументов (лишнее слово это `ExtraArgument`). `COMMAND_NAMES` дописан в конце именем `stats` (всего 15).

3. **social-core, `notify/`.**
   - `Notification::NewPost { author: UserId, post: PostId }`. `Display`: `новый пост [7] от id 1`, `to_push(to)` даёт `Push::NewPost { to, author, post }`. `Notification::for_event` для `Event::NewPost` по-прежнему `None`: получателей много, их считает рассылка.
   - `Notifiers` сам реализует `Notifier`: `notify(user, notification)` раздаёт уведомление всем уведомителям реестра по порядку, а `deliver(event)` пользуется им.

4. **social-core, `domain/` и `network/`.** `User` получает `Clone`. `Network::audience(post: PostId) -> Vec<UserId>`: подписчики автора, которым пост виден (`can_view`), без тех, кто скрыл автора (`mute`), в порядке подписки; удалённый или неизвестный пост никому. Сам автор в аудиторию не входит.

5. **social-core, `shared/`: рассылка, счётчики и кэш профилей.** `SharedNetwork` остаётся клонируемой ручкой, все клоны делят одну сеть, один пул, одни счётчики и один кэш.
   - `shared/fanout.rs`: пул рассылки на `FANOUT_WORKERS: usize = 4` воркера (`social_core::FANOUT_WORKERS`) запускается в `SharedNetwork::new` и `with_notifiers`; не запустился, значит `Error::Start`.
   - `SharedNetwork::publish`: под замком сети публикует пост и берёт `audience`, отпускает замок, ставит в пул по заданию на каждого получателя (задание раздаёт `Notification::NewPost` всем уведомителям реестра) и сразу отвечает. Ни доставка, ни ожидание доставки не идут под замком сети: уведомитель вправе сам спросить сеть (`profile`, `read`) и не должен на этом повиснуть. Подписки, лайки и сообщения идут как раньше, через шину событий: у них один получатель, это дёшево.
   - `SharedNetwork::flush(&self)` ждёт, пока пул рассылки доставит всё поставленное. `inbox(user)` сначала делает `flush`, поэтому в ящике всё, что опубликовано до вызова. Порядок уведомлений о разных постах у одного получателя не гарантирован: их разносят разные воркеры.
   - `Stats { pub posts: u64, pub notifications: u64 }` (`Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Default`, `social_core::Stats`) и `SharedNetwork::stats(&self) -> Stats`: `posts` это успешные `publish`, `notifications` это доставленные уведомления, по одному на получателя, сколько бы уведомителей ни было (подписки, лайки, сообщения и новые посты). Счётчики атомарные (`AtomicU64` в `Arc`, для счётчиков хватает `Relaxed`), `stats` не трогает замок сети.
   - `shared/profiles.rs`: `SharedNetwork::profile(&self, id: UserId) -> Option<Arc<User>>` с кэшем на `RwLock<HashMap<UserId, Arc<User>>>`. Попадание отдаётся под read-замком кэша, без замка сети, и это всегда один и тот же `Arc`. Промах берёт копию пользователя под замком сети, отпускает его и только потом под write-замком кладёт через `entry().or_insert`, поэтому одновременные промахи получают один `Arc`. Неизвестный id это `None`, и он не кэшируется.
   - Замки не вкладываются друг в друга: сеть, кэш и очередь пула берутся по одному. Тогда спорить о порядке захвата некому, и взаимной блокировке неоткуда взяться.
   - Регистрация остаётся атомарной: проверка «ник свободен», выдача id и вставка идут в одном `Network::register` под одним захватом. Не разноси их, когда будешь трогать замки.

6. **social-core, `commands/`.** `inbox` показывает новый пост: `@alice опубликовал пост [1]`. Ники в `inbox` берутся из кэша профилей (`profile`). `stats` делает `flush` и отвечает `постов: 3, уведомлений: 7`.

7. **tools/repl.** В режиме `--json` после ответа на строку терминал делает `flush` и только потом отдаёт накопленные push: push о новом посте идут сразу за ответом на `posts.create`. При выходе терминал дожидается рассылки (это делает `Drop` пула), лог `--log` не обрывается.

8. **Старые тесты.** Веха кладёт новые версии четырёх тестов прошлых вех: `contracts/tests/m11_events_notifiers.rs` и `contracts/tests/m14_indexes_trends.rs` (проверяют начало `COMMAND_NAMES`, теперь там 15 команд), `social-core/tests/m13_wire_contracts.rs` и `tools/repl/tests/m13_wire_contracts.rs` (подписчик bob получает push `new_post`).

## Дерево после вехи

```
backend/
  Cargo.toml                        + worker-pool в [workspace.dependencies]
  src/crates/
    worker-pool/
      Cargo.toml                    без зависимостей
      src/
        lib.rs                      только mod и pub use
        pool.rs                     ThreadPool: очередь под Mutex, два Condvar, воркеры, Drop
        handle.rs                   JobHandle<T>: результат через канал
        error.rs                    PoolClosed, JobError
      tests/
        m15_background_fanout.rs    тесты курса
    contracts/
      src/
        push/mod.rs                 + Push::NewPost
        command/mod.rs              + stats
      tests/
        m15_background_fanout.rs    тесты курса
        m11_events_notifiers.rs     новая версия
        m14_indexes_trends.rs       новая версия
    social-core/
      Cargo.toml                    + worker-pool
      src/
        lib.rs                      + FANOUT_WORKERS, Stats
        domain/user.rs              User: Clone
        network/posts.rs            + audience
        network/follows.rs          + список подписчиков
        notify/mod.rs               + Notification::NewPost, Notifier для Notifiers
        shared/
          mod.rs                    SharedNetwork: publish с рассылкой, flush, stats, profile
          fanout.rs                 пул рассылки, FANOUT_WORKERS, Stats, атомарные счётчики
          profiles.rs               кэш профилей на RwLock
        commands/mod.rs             + stats, новый пост в inbox
      tests/
        m15_background_fanout.rs    тесты курса
        m13_wire_contracts.rs       новая версия
  tools/repl/
    src/run/mod.rs                  flush перед push в --json
    tests/
      m15_background_fanout.rs      тесты курса
      m13_wire_contracts.rs         новая версия
```

Остальное дерево не меняется.

## Приёмка

- `src/crates/worker-pool/tests/m15_background_fanout.rs`: `submit` и `join`, четыре задания идут одновременно на четырёх воркерах, паника задания не убивает воркер, `wait_idle` ждёт и работающие задания, `pending`, `close` отказывает и доделывает очередь, `Drop` доделывает очередь и ждёт воркеры;
- `src/crates/contracts/tests/m15_background_fanout.rs`: push `new_post` на проводе, команда `stats`, `COMMAND_NAMES`;
- `src/crates/social-core/tests/m15_background_fanout.rs`: кому приходит новый пост (видимость, mute), `inbox` дожидается рассылки, `publish` и сеть не ждут зависшую доставку, рассылка идёт в `FANOUT_WORKERS` потоков, уведомитель может сам читать сеть, точные счётчики из многих потоков, `stats` и кэш профилей не ждут замок сети, один `Arc` на профиль, восемь одновременных регистраций одного ника, мягкая остановка при `drop` последней ручки, `inbox` и `stats` командами;
- `tools/repl/tests/m15_background_fanout.rs`: новый пост и `stats` в терминале, push `new_post` сразу после ответа в `--json`, полный лог `--log` при выходе;
- остальные тесты прошлых вех проходят;
- `cargo make check` зелёный.

Все ожидания в тестах идут с таймаутами: взаимная блокировка покажет «похоже на зависание», а не повесит проверку.

Архитектура: крейт `worker-pool` в воркспейсе, без зависимостей и комментариев, файлы по дереву, в пуле `Condvar` и ни одного `sleep`; в `shared/` пул рассылки на `ThreadPool`, атомарные счётчики, кэш на `RwLock` и ни одного своего потока; нигде нет `static mut`; и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
printf 'register alice\nregister bob\nregister carol\nfollow bob alice\nfollow carol alice\npost alice Пул заработал\ninbox bob\nstats\n' | cargo run -q -p repl -- --log
```

В stdout ответы терминала, в stderr лог уведомлений: две подписки у alice и по новому посту у bob и carol (эти две строки в любом порядке).
