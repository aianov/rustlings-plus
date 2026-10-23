# Веха 18. Асинхронная шина

Ядро уже живёт в tokio, но его части всё ещё переговариваются по-старому. Рассылка запускает по задаче на каждого получателя, и если подписчиков у звезды сто тысяч, в рантайме сто тысяч задач, а порядок постов у читателя случайный. Тренды узнают о лайке только по тикеру раз в минуту. Админ хочет на ходу урезать длину постов во время флешмоба, а для этого сейчас нужен перезапуск.

Эта веха проводит внутри ядра настоящие каналы. Доменные события расходятся через `broadcast`, и отстающий подписчик честно узнаёт, сколько пропустил. Уведомления встают в одну очередь с ёмкостью: переполнилась, значит публикующий подождёт. Настройки живут в `watch` и меняются командой. Обработчики крутятся в `select!` по каналам, таймерам и сигналу остановки, а карточка профиля собирается из четырёх частей одновременно под общим дедлайном.

## Что сделать

1. **contracts.**
   - Модуль `src/setting/mod.rs`: `Setting { PostLimit, TrendsWindow }` (`Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`), `name()` и `Display` дают `post_limit` и `trends_window`, `FromStr` разбирает их же, иначе `ParseError::UnknownSetting(String)` с текстом `неизвестная настройка "theme", есть post_limit, trends_window`. `SETTING_NAMES = ["post_limit", "trends_window"]`. Doc-тест на `Setting`.
   - Команды `set <настройка> <значение>` (`Command::Set { setting: Setting, value: u64 }`; без настройки `MissingArgument("настройка")`, без значения `MissingArgument("значение")`, не число `BadNumber`) и `card <кто> <чей>` (`Command::Card { who: Username, whose: Username }`, без второго ника `MissingArgument("чья карточка")`). `COMMAND_NAMES` дописан в конце именами `set` и `card` (всего 18).

2. **social-core, `events/bus.rs`: асинхронная шина.** `App` держит `broadcast::Sender<Arc<Event>>` на `AppConfig::events` мест и подписывает на `Topic::All` синхронной шины сети обработчик, который кладёт туда `Arc::new(*event)` (без получателей ошибка отправки не важна). `App::events() -> EventStream`. `EventStream::next(&mut self) -> Next`: `Next::Event(Arc<Event>)`, `Next::Missed(n)` при `RecvError::Lagged(n)` (пропуски копятся в `missed()`) и `Next::Closed`, когда ушёл последний `App`. `next_event()` пропускает потери и отдаёт `Option<Arc<Event>>`. Экспорт: `social_core::{EventStream, Next}`. Один `Arc` на событие у всех подписчиков.

3. **social-core, `app/fanout.rs`: очередь доставки.** Вместо задачи на получателя одна очередь `mpsc::channel(AppConfig::queue)` и одна задача доставки. `publish` после замка сети кладёт по уведомлению на получателя через `send().await`: очередь полна, значит `publish` ждёт (backpressure). `flush` кладёт в ту же очередь метку с `oneshot::Sender<()>` и ждёт ответа не дольше `op_timeout`: всё, что встало в очередь раньше метки, к ответу уже доставлено. Порядок уведомлений у получателя теперь совпадает с порядком публикации. Задача доставки крутится в `select!` с `biased;`: сначала сигнал остановки, потом очередь. Паника уведомителя не убивает доставку (`catch_unwind`).

4. **Сигнал остановки.** В ядре `watch::Sender<()>`, обработчики держат `watch::Receiver<()>` и выходят, когда `changed()` вернул ошибку: отправитель ушёл вместе с последним `App`. Фоновые задачи держат ядро только через `Weak`, иначе оно не умрёт никогда.

5. **social-core, `app/settings.rs`: живые настройки.** `Settings { post_limit: usize, trends_window: u64 }` (`pub`, `Copy`, `Eq`, `Default` = `MAX_POST_LEN` и `TRENDS_WINDOW`) в `watch::Sender<Settings>`.
   - `settings() -> Settings` (копия, `Ref` не живёт дольше строки), `watch_settings() -> watch::Receiver<Settings>`;
   - `set(setting, value) -> Result<Settings, AppError>` через `send_modify`; `post_limit` от 1 до `MAX_POST_LEN`, `trends_window` от 1, иначе `AppError::BadSetting { setting, value }` с текстами `post_limit от 1 до 280, а не 0` и `trends_window от 1 секунды, а не 0` (коды `rejected` и 422, настройка не меняется);
   - `publish` проверяет живой предел: если `post_limit < MAX_POST_LEN` и в тексте без пробелов по краям больше символов, это `DomainError::PostOverLimit(limit)` (`пост длиннее 10 символов: такой предел сейчас поставил админ`). Предел `MAX_POST_LEN` по-прежнему проверяет сама сеть (`PostTooLong`);
   - окно трендов берётся из `settings().trends_window`.

6. **social-core, `app/trends.rs`: обработчик трендов.** Одна фоновая задача держит тикер, запросы и шину событий в одном `select!` с `biased;`:
   - остановка (выход);
   - запросы `refresh_trends`: `App::refresh_trends()` кладёт запрос с `oneshot` в `mpsc::channel(8)` и ждёт ответа, всё не дольше `op_timeout` (`Timeout { op: Op::Trends, .. }`, закрытый канал это `Stopped(Op::Trends)`);
   - смена настроек (`settings.changed()`) сразу пересчитывает тренды;
   - тикер `interval_at` со `Skip`, как в m17;
   - события: `Liked` (и потери `Missed`, о которых не узнать, были ли там лайки) взводят пересчёт через `trends_debounce` тишины, каждый новый лайк отодвигает таймер (`Sleep::reset`), ветка таймера выключена предусловием, пока лайков не было. Потери обработчика копятся в `App::missed_events()`.
   - Пересчёт как в m17: копия лайков под замком, топ в `run_heavy(Op::Trends, ..)`.

7. **social-core, `app/card.rs`: карточка профиля.** `App::card(viewer, whose) -> Result<Card, AppError>`, `Card { id, name, bio, followers, following, posts, likes, recent: Vec<PostId>, followed }` (`pub`, `Clone`, `Eq`). Четыре части (`async fn`): профиль из кэша (`UnknownUser`, если нет), счётчики, `CARD_POSTS = 3` последних поста через страницу автора (видимость для `viewer`), подписан ли `viewer`. Собираются через `try_join!` под одним `timeout(op_timeout)` (`Timeout { op: Op::Card, .. }`, `Op::Card` пишется `карточка профиля`).

8. **AppConfig** получает `trends_debounce: Duration` (2 с), `events: usize` (256) и `queue: usize` (1024). Экспорт из корня: `Card`, `CARD_POSTS`, `Settings`.

9. **Команды.** `set post_limit 100` отвечает `post_limit = 100`. `card <кто> <чей>`:
   ```
   @alice
   подписчиков 2, подписок 1, постов 4, лайков 2
   ты подписан
   [4] четыре (лайков: 1)
   [3] три (лайков: 0)
   [2] два (лайков: 0)
   ```
   Первая строка `@ник: био`, если био есть; третья `ты подписан`, `ты не подписан` или `это ты`. Отрисовка карточки живёт в `commands/render.rs`.

10. **Старые тесты.** Веха кладёт новые версии `social-core/tests/journey/m15_background_fanout.rs` (без проверки параллельной доставки двум подписчикам: доставка теперь одна упорядоченная очередь, пул воркеров с разбиением по получателю придёт в m20) и `contracts/tests/journey/m17_async_core.rs` (проверяет `import` на своём месте, а не длину `COMMAND_NAMES`).

## Дерево после вехи

```
backend/
  src/crates/
    contracts/
      src/
        lib.rs                      + Setting, SETTING_NAMES
        setting/mod.rs              Setting, doc-тест
        command/mod.rs              + set, card
        command/error.rs            + UnknownSetting
      tests/journey/
        m18_async_events.rs         тесты курса (и строка mod в tests/journey/main.rs)
        m17_async_core.rs           новая версия
    social-core/
      src/
        lib.rs                      + EventStream, Next, Card, CARD_POSTS, Settings
        events/
          mod.rs                    + mod bus
          bus.rs                    EventStream, Next
        domain/error.rs             + PostOverLimit
        app/
          mod.rs                    broadcast, watch, сигнал остановки, запуск обработчиков
          config.rs                 + trends_debounce, events, queue
          error.rs                  + Op::Card, AppError::BadSetting
          fanout.rs                 очередь доставки, flush через oneshot
          settings.rs               Settings, settings, watch_settings, set
          trends.rs                 обработчик трендов в select!
          card.rs                   Card, try_join! под дедлайном
        commands/
          mod.rs                    + set, card
          render.rs                 + карточка
          response.rs, error/mod.rs + BadSetting
      tests/journey/
        m18_async_events.rs         тесты курса
        m15_background_fanout.rs    новая версия
  tools/repl/tests/journey/
    m18_async_events.rs             тесты курса
```

Остальное дерево не меняется.

## Приёмка

- `src/crates/social-core/tests/journey/m18_async_events.rs`: события по порядку и один `Arc` двум подписчикам, `Missed(6)` при буфере на 4 и потери обработчика, поток закрывается без `App`, порядок уведомлений у подписчика, полная очередь держит `publish`, живые настройки и их проверка, окно трендов из настроек и пересчёт по смене настройки, пересчёт после тишины без тика, `refresh_trends` через `oneshot`, карточка профиля и ошибки её частей, команды `card` и `set`;
- `src/crates/contracts/tests/journey/m18_async_events.rs`: `Setting`, `set`, `card`, `COMMAND_NAMES`;
- `tools/repl/tests/journey/m18_async_events.rs`: `set` и `card` в терминале;
- остальные тесты прошлых вех проходят;
- `cargo make check` зелёный.

Все ожидания в тестах с таймаутами, таймеры на замороженном времени.

Архитектура: есть `events/bus.rs` с `broadcast` и `Lagged`; нигде нет `unbounded_channel`; `mpsc::channel` в доставке; `select!` с `biased;` и `oneshot` в доставке и в обработчике трендов; `watch` в настройках; `try_join!` под `timeout` в карточке; и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
printf 'register alice\nregister bob\nfollow bob alice\npost alice Флешмоб начался!\nset post_limit 10\npost alice Очень длинный пост про флешмоб\npost alice Коротко\nlike bob 1\ncard bob alice\n' | cargo run -q -p repl
```

Второй пост alice упирается в новый предел, третий проходит, а карточка показывает её счётчики и последние посты.
