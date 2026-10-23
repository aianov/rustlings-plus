# Веха 16. Тесты и замеры

Соцсеть выросла, и тесты к ней тоже. Только тест про тренды за сутки сейчас врёт: лайк ставится «сейчас» по настоящим часам, и проверить, что через сутки он выпал из окна, можно, только прождав сутки. А кто скажет, быстрая ли у нас лента? «Вроде шустрая» это не число. Один `feed` сейчас просит у кучи по три-четыре строки на каждый пост, и никто этого не видит.

Эта веха даёт соцсети свой испытательный стенд. Время ядро теперь спрашивает у часов, которые можно подменить. Фейки, часы, журнал уведомлений, счётчик аллокаций и замер живут в отдельном крейте `test-support`, который видят только тесты и примеры, а в релиз он не попадает. Счётчик сразу находит лишние аллокации в ленте, а `cargo xtask bench` печатает медиану и p99 её построения.

## Что сделать

1. **contracts: часы.** Модуль `src/clock/mod.rs`:
   - `pub trait Clock: Send + Sync { fn now(&self) -> Timestamp; }` (секунды с 1970 года, как `Timestamp` из m14);
   - `SystemClock` (unit-структура, `Debug`, `Clone`, `Copy`, `Default`) отдаёт настоящее время через `SystemTime`;
   - оба реэкспортированы из корня: `contracts::{Clock, SystemClock}`.

2. **contracts: doc-тесты.** У публичного API есть исполняемые примеры (`/// ```` над элементом, без `ignore`): `normalize_name` или `Username` (`username/mod.rs`), `UserId` или `PostId` (`ids/mod.rs`), `Cursor` (`cursor/mod.rs`), `Envelope` или `decode` (`envelope/mod.rs`), `Clock` или `SystemClock` (`clock/mod.rs`). Пример показывает настоящий вызов и проверяет результат через `assert!`/`assert_eq!`; `cargo test` запускает их вместе с остальными тестами.

3. **social-core: время через `Clock`.**
   - `domain/clock.rs` с функцией `now()` уходит. В `social-core/src` больше нет ни одного `SystemTime`.
   - `Network` держит часы в поле `Box<dyn Clock>`, по умолчанию `SystemClock` (в `new` и `with_stores`).
   - `Network::with_clock(self, clock: impl Clock + 'static) -> Network<U, P>` подменяет часы: `MemNetwork::new().with_clock(clock)`.
   - `Network::now(&self) -> Timestamp` спрашивает свои часы. `publish` и `like` берут время у них (`publish_at` и `like_at` остаются).
   - Команда `trends` считает окно от `network.now()`, а не от системных часов.
   - `social_core::{Clock, SystemClock}` реэкспортированы из корня ядра.

4. **social-core: лента без лишних аллокаций.** Текстовые ответы команд со списками (`feed`, `page`, `posts`, `thread`, `trends`, `inbox`, история сообщений) пишутся в одну `String` с запасом (`String::with_capacity`, `write!`), без `Vec<String>` и `join`. Ник автора выводится через `Display`-обёртку (`Name`), без своей `String` на строку. Это живёт в `commands/render.rs`. Страница из `pages(n)` сразу резервирует место: `Vec::with_capacity(n.min(сколько ещё может дать итератор))`, верхняя граница берётся из `size_hint`. Текст ответов не меняется ни на символ.

   Бюджет: `feed <ник> <N>` и `page <ник> <номер> <N>` при коротких постах делают **не больше 5 аллокаций на всю команду** при любом N (тест берёт 5, 20 и 50). Сюда входит и разбор строки команды.

5. **Новый крейт `src/crates/test-support`.** Пакет `test-support` (библиотека `test_support`), общие поля из воркспейса, без комментариев. Корень объявляет его в `[workspace.dependencies]`: `test-support = { path = "src/crates/test-support" }`.
   - Свои `[lints]` вместо общих: `[lints.rust] unsafe_code = "deny"` и `[lints.clippy] unwrap_used = "deny"`. Общий `forbid` нельзя снять даже точечно, а `deny` снимается одним `#[allow(unsafe_code)]` над `unsafe impl GlobalAlloc`. Больше unsafe нигде в крейте нет.
   - `[features] test-util = ["dep:contracts", "dep:social-core"]`, обе зависимости `optional = true`. Всё, что строится из типов соцсети (часы, id, хранилища, журнал уведомлений), объявлено в `lib.rs` под `#[cfg(feature = "test-util")]`. Счётчик аллокаций и замер от соцсети не зависят и доступны всегда.
   - `alloc/`: `CountingAlloc` (unit-структура) реализует `GlobalAlloc` поверх `System` и считает `alloc`, `alloc_zeroed` и `realloc` в `thread_local!` счётчике текущего потока (`Cell<usize>`). `allocations<R>(f: impl FnOnce() -> R) -> (R, usize)`: результат `f` и сколько аллокаций сделал этот поток, пока она работала. Подключает аллокатор сам тестовый файл: `#[global_allocator] static ALLOC: CountingAlloc = CountingAlloc;`.
   - `bench/`: `Summary` (`Debug`, `Clone`, `PartialEq`, `Eq`) из замеров `Duration`. `Summary::from_samples(Vec<Duration>) -> Option<Summary>` (пусто, значит `None`), `runs()`, `percentile(p: u32) -> Duration` по ближайшему рангу (отсортированные замеры, элемент с номером `ceil(p * n / 100)`, считая с единицы; `p` зажимается в 1..=100), `median()` это `percentile(50)`, `p99()` это `percentile(99)`. `Display`: `медиана 50.0 мкс, p99 99.0 мкс, замеров 100` (микросекунды с одним знаком). `measure<R>(warmup: usize, runs: usize, f: impl FnMut() -> R) -> Option<Summary>`: `warmup` прогонов вхолостую, потом `runs` замеров через `Instant`, результат каждого вызова идёт в `std::hint::black_box`.
   - `clock/`: `FakeClock` (`Debug`, `Clone`, `Default`) реализует `Clock`. `new(start)`, `advance(secs)`, `set(at)`. Клоны делят одну стрелку (`Arc<AtomicU64>`): сеть держит один клон, тест двигает другой. Сами часы не идут.
   - `ids/`: `SeqIds::starting_at(first: u64)`, `next_id(&self) -> u64` выдаёт `first`, `first + 1`, ... без повторов и из многих потоков.
   - `stores/`: `FakeUsers::starting_at(first)` и `FakePosts::starting_at(first)` реализуют `UserStore` и `PostStore`: id выдают из `SeqIds` с заданного начала, хранят во внутренних `MemUserStore`/`MemPostStore`. `FakePosts::scans(&self) -> Scans` отдаёт счётчик полных обходов: каждый вызов `all()` это один обход, а `get`, `get_mut`, `insert` и `count` обходом не считаются. `Scans` (`Debug`, `Clone`, `Default`): `get(&self) -> usize`, клоны видят один счётчик.
   - `notify/`: `RecordingNotifier` (`Debug`, `Default`) реализует `Notifier` и записывает каждый вызов: `new()`, `calls() -> Vec<(UserId, Notification)>` в порядке поступления, `count()`, `for_user(user) -> Vec<Notification>`. Пишут в него из многих потоков.

6. **social-core: test-support только для тестов.** В `[dev-dependencies]` ядра строка `test-support = { workspace = true, features = ["test-util"] }`, в `[dependencies]` его нет. Обычная зависимость тут и не выйдет: test-support сам зависит от ядра, и cargo скажет про цикл. А вот dev-зависимость в обратную сторону разрешена, и интеграционные тесты в `tests/` видят тот же `social_core`, что и test-support. Терминал (`tools/repl`) про test-support не знает совсем.

7. **social-core: свои интеграционные тесты.** `tests/common/mod.rs` с помощниками на `FakeClock` (например, сеть на замороженных часах и регистрация по нику) и `tests/clock.rs`, который начинается с `mod common;` и проверяет время сам: пост получает время часов сети, лайк выпадает из окна трендов ровно через `TRENDS_WINDOW`. Файла `tests/common.rs` нет: cargo сделал бы из него отдельный пустой тест.

8. **Замер ленты.** Пример `social-core/examples/feed_bench.rs` строит большую сеть на `FakeClock` (сотни пользователей, десятки тысяч постов, подписки), меряет команду `feed` через `test_support::measure` и печатает сводку. `cargo xtask bench` (`tools/xtask/src/bench.rs`) запускает его из корня воркспейса: `cargo run --release --quiet --package social-core --example feed_bench`, тем cargo, что лежит в переменной `CARGO` (как `check`). Упал пример, падает и `xtask bench`. Справка xtask называет `check`, `bench` и `new-service`.

## Дерево после вехи

```
backend/
  Cargo.toml                        + test-support в [workspace.dependencies]
  src/crates/
    contracts/
      src/
        lib.rs                      + Clock, SystemClock
        clock/mod.rs                Clock, SystemClock, doc-тесты
        username/mod.rs             + doc-тест
        ids/mod.rs                  + doc-тест
        cursor/mod.rs               + doc-тест
        envelope/mod.rs             + doc-тест
    social-core/
      Cargo.toml                    + [dev-dependencies] test-support
      src/
        lib.rs                      + Clock, SystemClock
        domain/mod.rs               clock.rs больше нет
        network/mod.rs              + часы: with_clock, now
        network/posts.rs            publish и like по часам сети
        feed/pages.rs               страница резервирует место
        commands/
          mod.rs                    ответы пишутся через render
          render.rs                 Name, одна строка ответа с запасом
      examples/
        feed_bench.rs               замер ленты
      tests/
        common/mod.rs               твои помощники на FakeClock
        clock.rs                    твои тесты времени
        m16_test_support.rs         тесты курса
    test-support/
      Cargo.toml                    свои [lints], feature test-util
      src/
        lib.rs                      только mod и pub use, фейки за test-util
        alloc/mod.rs                CountingAlloc, allocations
        bench/mod.rs                Summary, measure
        clock/mod.rs                FakeClock
        ids/mod.rs                  SeqIds
        stores/
          mod.rs
          users.rs                  FakeUsers
          posts.rs                  FakePosts, Scans
        notify/mod.rs               RecordingNotifier
      tests/
        m16_test_support.rs         тесты курса
  tools/xtask/
    src/
      main.rs                       + bench
      bench.rs                      cargo run --release ... --example feed_bench
      root.rs                       + cargo(): путь к cargo из CARGO
    tests/
      m16_test_support.rs           тесты курса
```

Остальное дерево не меняется.

## Приёмка

- `src/crates/test-support/tests/m16_test_support.rs`: `FakeClock` стоит, пока его не двинут, и клоны делят время; `SeqIds` с заданного начала и без повторов из восьми потоков; фейковые хранилища выдают id с заданного начала и считают полные обходы; сеть на фейках работает как на памяти; `RecordingNotifier` хранит вызовы по порядку и не теряет их из четырёх потоков; `allocations` считает alloc и realloc и только своего потока; `Summary` по ближайшему рангу на 100, 101 и одном замере, пусто это `None`; `measure` делает прогрев отдельно от замеров;
- `src/crates/social-core/tests/m16_test_support.rs`: публикация и `now` по часам сети, без `with_clock` это `SystemClock`, страница автора по времени часов, `trends` по часам сети, id из фейковых хранилищ доходят до ответов команд, страница автора и поиск по тегу не обходят хранилище, `RecordingNotifier` в реестре видит подписку, новый пост, лайк и сообщение, бюджет аллокаций `feed` и `page`, test-support только в `[dev-dependencies]` ядра и не в терминале;
- `tools/xtask/tests/m16_test_support.rs`: `xtask bench` один раз зовёт `cargo run --release -p social-core --example feed_bench` из корня и пропускает его вывод, падение замера роняет `xtask bench`, справка называет `bench`;
- doc-тесты contracts и твои `tests/clock.rs` проходят;
- остальные тесты прошлых вех проходят;
- `cargo make check` зелёный.

Все ожидания в тестах идут с таймаутами: зависание покажет «похоже на зависание», а не повесит проверку.

Архитектура: крейт `test-support` в воркспейсе, зависит только от contracts и social-core, свои `[lints]` с `unsafe_code = "deny"`, unsafe только в `alloc/`, счётчик в `thread_local!`, замер через `Instant` и `black_box`, фейки за `test-util`, без комментариев; ядро держит `dyn Clock` и не знает `SystemTime`; test-support подключён к ядру только в `[dev-dependencies]`; `tests/common/mod.rs` вместо `tests/common.rs`; doc-тесты в contracts без `ignore`; и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo test -p social-core --test clock
cargo xtask bench
```

`cargo xtask bench` первый раз собирает релиз (LTO, это минута-другая), потом печатает что-то вроде `медиана 8.2 мкс, p99 12.4 мкс, замеров 300`. Числа твои, зависят от машины; интересно сравнить их с тем, что было до правок ленты.
