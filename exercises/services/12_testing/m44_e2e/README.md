# Веха 44. Сквозные тесты

У соцсети шесть сервисов, шлюз и брокер, а проверяют их тесты, каждый из которых поднимает свой кусок по-своему. Стенд шлюза из `support.rs` знает всё про всех, но живёт внутри тестов шлюза, и его не позовёшь из другого места. Тесты ждут событий циклом со `sleep(20 мс)`: на медленной машине это падение, на быстрой это лишние секунды. Формат событий в NATS никто не фиксирует: поменял поле в `PostCreated`, и feed-service узнает об этом в продакшене. А связи «подписан, заглушён, аудитория» проверены десятком ручных примеров, хотя ошибка обычно прячется в порядке операций, которого никто не придумал.

Эта веха даёт соцсети сквозные тесты. Пакет `tests/e2e` поднимает auth, user, post, feed, chat, notify и шлюз в одном процессе на портах 0, ждёт готовности по `/health` и чисто всё останавливает. У каждого стенда свой корень тем и stream'ов NATS, а после остановки брокер курса пуст. Ожидание везде через `eventually`, без `sleep`. Крейт `test-support` возвращается с другим содержимым: `eventually`, строители пользователей и постов, расписание отказов для обёрток портов и детерминированный генератор. Форматы всех одиннадцати событий лежат золотыми файлами, потребители проверяют, что находят в событиях нужные им поля, а подписки сверяются с простой моделью на случайных последовательностях.

Всё собирается из главы 12_testing: задачи с svtest1 по svtest10 и проект svtest_e2e.

## Что сделать

1. **Крейт `src/crates/test-support`** (зависимость только tokio):
   - `eventually/`: `Eventually { deadline, first, cap }` (по умолчанию 10 с, 5 мс, 200 мс), `Eventually::within(deadline)`, `until(what, check) -> Result<T, GaveUp>`: `check` это замыкание, которое возвращает future с `Result`; попытки идут с паузой, растущей вдвое до `cap`, последняя пауза не выходит за срок; `GaveUp { what, deadline, attempts, last }` с понятным `Display`; `eventually(what, check)` это то же со значениями по умолчанию; крейт не паникует, решает вызывающий;
   - `builders/`: `PASSWORD`, `unique_name(prefix)` (буквы и цифры префикса в нижнем регистре, не длиннее 12, плюс номер процесса и общий атомарный счётчик; пустой префикс это `user`), `user() -> NewUser` с `named`, `exactly`, `password`, `display_name`, `bio`, `credentials()` (JSON для `/auth/register` и `/auth/login`) и `profile_patch()` (`None`, если менять нечего); `post() -> NewPost` с уникальным текстом, `text`, `tagged`, `body()`; строки в JSON экранируются;
   - `faults/`: `Fault { Never, Always, Next(n), Calls(номера), Every(n) }`, `Faults` (клонируется, клоны делят расписание): `new`, `calls_failing(&[..])`, `set`, `heal`, `hit() -> bool` (номера вызовов с единицы), `calls()`, `failed()`;
   - `random/`: `Rng::seeded(seed)` (splitmix64: `next_u64`, `below`, `index`, `chance(percent)`), `seed()` из `E2E_SEED` или из часов;
   - `roots/`: `unique_root(prefix)`: корень тем NATS, свой у каждого вызова.

2. **Пакет `tests/e2e`** (член воркспейса, библиотека и тесты):
   - `brokers/`: `nats_url()` (`RUSTLINGS_NATS_URL` или адрес по умолчанию), `nats(name)`, `drop_streams(&topics)`: удаляет stream'ы событий, чата, задач, мёртвых задач и бакеты присутствия, контрольных точек, саг и аренд (`_SWEEP`, `_DIGEST`, `_ORPHANS`) и ждёт, пока брокер подтвердит, что их нет;
   - `stack/`: `Stack::start()` и `start_with(StackConfig { gateway, digest_every })`: все слушатели на `127.0.0.1:0` до запуска сервисов, корень `unique_root("e2e")`, папка файлов user-service своя на стенд, notify-service стартует первым (подписка на события), остальные задачами; `ready()` ждёт 200 от `/health` шлюза; `request`, `register`, `login`, `connect(token)`, `member(&NewUser) -> (UserView, Client)`; `stop()` отменяет токен, дожидается каждой задачи (зависшая или упавшая это паника с именем сервиса) и вызывает `drop_streams`;
   - `client/`: `Client` поверх WebSocket клиента курса: `send`, `call` (пуши и события ленты, пришедшие до ответа, откладываются), `answer::<T>` (`Result<T, ErrorBody>`), `ok::<T>`, `push`, `push_where(условие)` (неподходящие пуши остаются в очереди по порядку), `feed`;
   - `golden/`: `GOLDEN_DIR` (`tests/e2e/golden`), `golden(name, actual)`: с `UPDATE_GOLDEN=1` переписывает файл, иначе сравнивает и при расхождении называет первую разошедшуюся строку;
   - `flaky/`: `FlakyFiles::new(store, puts, reads)`: `FileStore`, у которого `put` и `rename` отказывают по расписанию `puts`, а `get` по расписанию `reads`;
   - `waiting/`: `eventually` для тестов: то же, что в test-support, но `GaveUp` превращается в панику с текстом;
   - Makefile.toml: `cargo make e2e` гоняет `cargo test -p e2e`.

3. **Тесты пакета** (приходят готовыми и проверяют твою библиотеку):
   - `scenario.rs`: регистрация, вход, WebSocket, подписка, пуш `Followed` автору, пост, пуш `NewPost` подписчику, пост в ленте; правка профиля через `PATCH /users/me` видна в `GET /users/<ник>`;
   - `events.rs`: у каждого из одиннадцати видов событий золотой файл, он читается обратно в тот же факт; потребители (лента и уведомления) находят нужные им поля; лишнее поле не ломает чтение, незнакомый вид пропускается, схема из будущего это ошибка;
   - `faults.rs`: отказ записи не оставляет ни ссылки, ни файла, и повтор проходит; сбой `rename` оставляет сироту, которую находит уборка; сбой чтения это ошибка, а не «файла нет»;
   - `model.rs`: двадцать раундов по шестьдесят случайных `follow` и `mute` (с самоподпиской и несуществующим пользователем) против модели на множествах: `changed`, `following`, `followers`, `muted` и `audience` совпадают; при падении печатаются seed и шаги, повтор через `E2E_SEED`.

## Дерево после вехи

```
backend/
  src/crates/test-support/
    Cargo.toml
    src/
      lib.rs
      eventually/mod.rs   Eventually, GaveUp, eventually
      builders/mod.rs     NewUser, NewPost, unique_name
      faults/mod.rs       Fault, Faults
      random/mod.rs       Rng, seed
      roots/mod.rs        unique_root
  tests/e2e/
    Cargo.toml
    golden/               одиннадцать событий в JSON
    src/
      lib.rs
      brokers/mod.rs
      stack/mod.rs
      client/mod.rs
      golden/mod.rs
      flaky/mod.rs
      waiting/mod.rs
    tests/
      scenario.rs
      events.rs
      faults.rs
      model.rs
```

Остальное дерево прежнее.

## Приёмка

Тестам нужен NATS курса с JetStream на `127.0.0.1:54222` (адрес из `RUSTLINGS_NATS_URL`).

- `test-support/tests/journey/m44_e2e.rs`: паузы `eventually` растут вдвое и укладываются в срок, отказ с последним ответом; уникальные ники и корни; строители и экранирование; расписания отказов и общие клоны; повторяемый генератор.
- `tests/e2e/tests/*.rs`: сценарий, события, отказы, модель (см. выше).
- тесты прошлых вех проходят, `cargo make check` и `cargo make e2e` зелёные, после них в брокере курса не остаётся stream'ов.

Архитектура: `test-support` и `tests/e2e` в воркспейсе с общими правилами, test-support зависит только от tokio и не паникует; папка `tests/e2e/golden` есть; в `tests/e2e/tests` нет `sleep(`; стенд на портах 0, со своим корнем, ждёт `/health` и убирает за собой stream'ы; `UPDATE_GOLDEN`; паузы `eventually` до потолка; seed в сообщениях модели; задача `e2e` в Makefile.toml; и все договорённости прошлых вех.

## Как запустить

NATS курса должен работать: `127.0.0.1:54222`.

```
cd backend
cargo make check
cargo make e2e
```

Клиент прежний: `cargo make demo`, `register alice correct-horse`, `login alice correct-horse`, во втором клиенте bob подписывается на alice, alice пишет пост, у bob приходит пуш. `cargo make e2e` прогоняет то же самое за пару секунд, а `E2E_SEED=42 cargo test -p e2e --test model` повторяет один и тот же случайный прогон.
