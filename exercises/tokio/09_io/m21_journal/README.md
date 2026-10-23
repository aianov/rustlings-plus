# Веха 21. Журнал

Соцсеть научилась уходить чисто, но всё, что в ней было, живёт только в памяти. Закрыл терминал, и alice, её подписчики, посты и переписка пропали, а следующий запуск начинается с пустого места. Базы данных у нас пока нет (она придёт в разделе data), но сохранить историю можно и проще: каждое изменение дописывается строкой в журнал, а при старте ядро читает журнал от начала и повторяет всё, что было.

Журнал это кусок настоящего ввода-вывода: буфер записи и правило сброса, явное закрытие, чтение строк с лимитом длины, обрыв посреди последней строки. Код журнала не знает про файлы, он пишет в любой `AsyncWrite` и читает из любого `AsyncRead`, поэтому тесты гоняют его на пробнике, на приёмнике, который берёт по пять байт, и на источнике, который отдаёт по три.

## Что сделать

1. **Корень.** У `tokio` в `[workspace.dependencies]` появляются фичи `io-util` и `fs`.

2. **contracts, формат журнала.**
   - `visibility/`: `Visibility { Public, Followers, Private }` переезжает сюда из ядра (serde, слова `public`, `followers`, `private`). Ядро реэкспортирует его, как ids и ник: `social_core::Visibility` остаётся тем же типом.
   - `journal/`: `Entry` (serde, `#[serde(tag = "event", rename_all = "snake_case")]`), одна запись на одно настоящее изменение:
     `Registered { user, name }`, `Followed { who, whom }`, `Muted { who, whom }`,
     `Posted { post, author, text, visibility, at }`, `Liked { post, by, at }`, `Deleted { post, by }`,
     `Commented { comment, post, author, text }`, `Replied { comment, parent, author, text }`, `MessageSent { from, to, text }`.
     Id на проводе строками (`u_1`, `p_1`, `c_1`), время `at` в секундах. `Entry::event()` и `ENTRY_EVENTS` (9 имён в порядке вариантов). Строка журнала это `write_line(&entry)`, разбор `decode::<Entry>(line)`: JSON по-прежнему пишет и читает только contracts.

3. **social-core, журнал** (`src/journal/`, папка с файлами `mod.rs`, `writer.rs`, `replay.rs`, `file.rs`, `error.rs`).
   - `JournalWriter<W: AsyncWrite + Unpin>`: `new(sink, flush_every)` (внутри `BufWriter`), `append(&Entry).await` (строка целиком через `write_all`, после каждой `flush_every`-й записи `flush`, не чаще и не реже), `unflushed()`, `close(self).await` (`flush`, затем `shutdown`).
   - `replay(source: impl AsyncRead + Unpin, max_line).await -> Result<Replayed, JournalError>`, `Replayed { entries, bytes, torn }`. Каждая строка читается через `take(max_line + 1)` и `read_until(b'\n')`: строка длиннее `max_line` байт это `TooLong { line, max }`, причём целиком её не читаем; последняя строка без `\n` это обрыв посреди записи, она отбрасывается (`torn = true`, `bytes` считает только целые строки); целая, но испорченная строка это `Bad { line, reason }`.
   - `file.rs` (только `tokio::fs`): `load(path, max_line).await -> Result<Vec<Entry>, JournalError>` (нет файла, значит пустой журнал; был обрыв, значит файл обрезается до `bytes` через `set_len`, иначе следующая запись приклеится к обрывку), `Journal::open(path).await` (создаёт папки, открывает с `create` и `append`).
   - `Journal::new(impl AsyncWrite + Send + Unpin + 'static)` прячет приёмник в `Box<dyn AsyncWrite + Send + Unpin>` (`Sink`).
   - `JournalError { Io, TooLong, Bad, Rejected { line, error: DomainError }, Diverged { line, expected, got } }`, `line()`, Display по-русски, `source`. `social_core::Error::Journal` (код `internal`).

4. **social-core, восстановление.** `Network::restore(&[Entry]) -> Result<usize, JournalError>` (`network/restore.rs`) повторяет записи по порядку обычными операциями сети (`register`, `follow`, `publish_at`, `like_at`, `comment`, ...). Id, который выдала сеть, обязан совпасть с id в записи, иначе `Diverged`; отказ домена это `Rejected`, у обоих номер строки. Восстановление идёт мимо `App`: никто не получает уведомлений второй раз, и журнал не пишется сам в себя.

5. **social-core, запись.** Порт отдаёт время изменения: `Published::at`, `Liked::at`. Операции записи `App` переезжают из `app/mod.rs` в `app/writes.rs` и после настоящего изменения кладут `Entry` в журнал (повторная подписка, второй лайк и отказы не пишутся). `App::journaled(repos, notifiers, config, journal)`: журнал пишет задача в отдельной стадии `journal`, она объявлена последней, поэтому гаснет последней: дочитывает очередь, сбрасывает и закрывает журнал (`app/journal.rs`). Порядок в журнале обязан совпадать с порядком, в котором хранилище выдало id, даже когда записи идут параллельно: изменение и постановка его записи в очередь идут под одним `tokio::sync::Mutex` (guard переживает `.await`, это ровно тот случай). Без журнала замок не берётся. После `close` запись отвечает `AppError::Stopped(Op::Journal)`. `App::restore(network, notifiers, config, path).await` (в `storage/mem/app.rs`): `load`, `restore`, `Journal::open`, `journaled`. `AppConfig` получает `journal_flush_every` (32) и `journal_max_line` (64 КиБ).

6. **social-core, импорт** читает файл через `tokio::fs`; `std::fs` в ядре больше нет.

7. **test-support** (под `test-util`): `Probe` (приёмник в памяти: `bytes`, `text`, `lines`, `writes`, `flushes`, `is_shut`), `Chunked::new(writer, n)` (берёт не больше `n` байт за `poll_write`), `Trickle::new(reader, n)` (отдаёт не больше `n` байт за раз и через раз отвечает `Pending`).

8. **tools/repl.** `repl::JOURNAL_ENV = "SOCIAL_JOURNAL"`: если переменная задана, терминал поднимает ядро через `App::restore` из этого файла и пишет в него же. `repl::run_at(mode, notifiers, journal: Option<&Path>, input, output)`, `Terminal::with_journal(notifiers, Option<&Path>)`; прежние `run` и `Terminal::with_notifiers` работают без журнала. В `.env.example` строка с переменной закомментирована: если включить её в `.env`, журнал общий у всех запусков, и тесты тоже начнут видеть чужих пользователей.

## Дерево после вехи

```
backend/
  Cargo.toml                        tokio + io-util, fs
  .env.example  .env                + # SOCIAL_JOURNAL=data/journal.ndjson
  src/crates/
    contracts/src/
      lib.rs                        + Entry, ENTRY_EVENTS, Visibility
      journal/mod.rs                Entry
      visibility/mod.rs             Visibility
    social-core/src/
      lib.rs                        + Journal, JournalError, JournalWriter, Replayed, Sink, load, replay
      domain/mod.rs                 Visibility из contracts
      journal/
        mod.rs                      Journal, Sink
        writer.rs                   JournalWriter
        replay.rs                   replay, Replayed
        file.rs                     load, Journal::open (tokio::fs)
        error.rs                    JournalError
      network/restore.rs            Network::restore
      ports/repo.rs                 + Published::at, Liked::at
      app/
        mod.rs                      journaled, стадия journal
        journal.rs                  очередь, порядок, задача записи
        writes.rs                   операции записи
        config.rs                   + journal_flush_every, journal_max_line
        error.rs                    + Op::Journal
        import.rs                   tokio::fs
      error/mod.rs                  + Error::Journal
      storage/mem/app.rs            + App::restore
    test-support/src/io/mod.rs      Probe, Chunked, Trickle
  tools/repl/src/
    main.rs                         путь журнала из окружения
    runtime/mod.rs                  + JOURNAL_ENV, open
    run/mod.rs                      + run_at
    terminal/mod.rs                 + with_journal
  */tests/journey/m21_journal.rs    тесты курса в contracts, social-core и tools/repl
```

Остальное дерево не меняется.

## Приёмка

- `contracts/tests/journey/m21_journal.rs`: форма записей в JSON, имена событий, одна запись на одну строку, неизвестное событие это ошибка;
- `social-core/tests/journey/m21_journal.rs`: сброс ровно по `flush_every` и при `close`, строки целы на кусочном приёмнике и кусочном источнике, обрыв отбрасывается и измеряется, строка на 8 МиБ отклоняется по лимиту, испорченная строка называет свой номер, ядро пишет в журнал каждое настоящее изменение и гасит журнал последним, 32 параллельные регистрации ложатся в порядке id, восстановленная сеть та же (id, лента, время поста, лайк, ветка, переписка, никаких повторных уведомлений), журнал не того ядра отклоняется, файл журнала переживает перезапуски и обрыв последней строки;
- `tools/repl/tests/journey/m21_journal.rs`: второй сеанс видит первый, путь берётся из `SOCIAL_JOURNAL`;
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву, `std::fs` в ядре нет, `writer.rs` и `replay.rs` не знают про `File`, запись через `BufWriter`, чтение через `take`, `Visibility` объявлен только в contracts, у ядра нет serde, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
export SOCIAL_JOURNAL=data/journal.ndjson
printf 'register alice\nregister bob\nfollow bob alice\npost alice Я переживу перезапуск\n' | cargo run -q -p repl
printf 'feed bob\n' | cargo run -q -p repl
cat data/journal.ndjson
```

Второй запуск начинает не с пустого места: лента bob на месте, а в `data/journal.ndjson` по строке JSON на каждое изменение.
