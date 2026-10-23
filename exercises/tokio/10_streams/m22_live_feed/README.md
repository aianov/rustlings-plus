# Веха 22. Живая лента

Лента у нас как утренняя газета: открыл, прочитал, закрыл. Чтобы узнать, что alice написала ещё, bob снова и снова набирает `feed`. Сделаем ленту живой: сначала последние посты, потом новые появляются сами, пока bob занят другим. Заодно лента учится ходить по страницам по курсору и подтягивать имена авторов параллельно, как будет, когда посты и профили разъедутся по разным сервисам.

Лента становится стримом. Его тянет потребитель: пока никто не просит следующий пост, хранилище никто не трогает, а брошенный стрим сам отписывается от событий. История и живые посты склеиваются в один поток так, чтобы пост, вышедший, пока грузилась первая страница, не потерялся и не пришёл дважды.

## Что сделать

1. **Корень.** В `[workspace.dependencies]` появляется `futures = { version = "0.3", default-features = false, features = ["std"] }`, его подключают social-core и tools/repl. `BoxFuture` портов остаётся своим, `async-trait` по-прежнему не нужен.

2. **contracts.** `ChatCommand::Watch { recent }` (`/watch [N]`: N от 1 до 100, без числа `DEFAULT_FEED_LIMIT`, ошибки числа и лишних слов как у `/history`) и `ChatCommand::Unwatch` (`/unwatch`, без аргументов). `CHAT_COMMAND_NAMES` дописан в конец: `/watch`, `/unwatch`.

3. **social-core, порт ленты.**
   - `FeedSlice { posts: Vec<PostItem>, next: Option<PostId> }` в `ports/repo.rs`.
   - `PostRepo::feed_page(viewer, before: Option<PostId>, size) -> Result<FeedSlice, DomainError>`: посты ленты зрителя (свои и подписок, с видимостью, без скрытых и удалённых) строго старше `before`, от новых к старым, не больше `size`; `next` это id последнего поста страницы, если за ним есть ещё; неизвестный зритель `UnknownUser`.
   - `PostRepo::feed_item(viewer, post) -> Option<PostItem>`: пост, если он в ленте зрителя.
   - Оба отдают посты без имени автора (`author_name` пустой): имена подтягивает ядро из кэша профилей. В сети это `Network::feed_before(viewer, before)` (итератор) и `Network::feed_post(viewer, id)`. `SlowRepos` из test-support замедляет и новые методы.

4. **social-core, стримы** (`src/feed/stream.rs`).
   - `FeedEvent { History(PostItem), UpToDate, Live(PostItem) }`.
   - `pages(fetch)`: страницы через `stream::unfold` по курсору, посты страниц разворачиваются в один поток `Result<PostItem, AppError>`; ошибка страницы идёт по стриму как `Err`, после неё страниц больше нет.
   - `with_authors(posts, lookup, lookups)`: имя автора к каждому посту, не больше `lookups` запросов сразу, порядок ленты сохраняется (`buffered`).
   - `new_posts(events, token)`: id новых постов из шины событий, стрим кончается по токену остановки.
   - `watch(history, live)`: сначала история, потом один `UpToDate`, потом живые посты, кроме тех, что не новее самого нового поста истории.

5. **social-core, фасад** (`src/app/live.rs`).
   - `App::feed_page`, `App::feed_item` (с именем автора через `profile`).
   - `App::feed_stream(viewer)`: страницы по `feed_page` постов, авторы по `feed_lookups` за раз.
   - `App::live(viewer)`: живые посты ленты зрителя. Подписка на события берётся сразу при вызове.
   - `App::watch(viewer, recent)`: `live` берётся до первой страницы, история это первые `recent` постов `feed_stream`.
   - `App::watchers()`: сколько живых стримов сейчас слушает события. Брошенный стрим уменьшает счётчик сам (Drop), после `close` живая лента кончается.
   - Все три стрима `Send` и `'static`: тип возврата `impl Stream<Item = ...> + Send + use<>`.
   - `AppConfig` получает `feed_page` (20) и `feed_lookups` (4).

6. **tools/repl.**
   - Автомат сессии: `Action::Watch { user, recent }` и `Action::Unwatch`; без сессии `NotLoggedIn`, сессию обе команды не меняют.
   - Терминал держит стрим `App::watch` вошедшего пользователя. `/watch` печатает историю (`@alice [2] два (лайков: 0)`, пустая лента: `лента пуста`) и строку `слежу за лентой @bob, новые посты появятся здесь`.
   - `Terminal::live() -> Vec<String>` забирает готовые события, не дожидаясь новых (`now_or_never`), строками `вживую: @alice [3] три (лайков: 0)`; `run` печатает их сразу после ответа на каждую команду.
   - `/unwatch` бросает стрим (`больше не слежу за лентой @bob`, без слежки `за лентой никто не следит`), `/logout` тоже. `Terminal::watching() -> Option<&Username>`.

## Дерево после вехи

```
backend/
  Cargo.toml                        + futures
  src/crates/
    contracts/src/chat/mod.rs       + Watch, Unwatch, CHAT_COMMAND_NAMES
    social-core/
      Cargo.toml                    + futures
      src/
        lib.rs                      + FeedEvent, FeedSlice
        feed/
          mod.rs                    + stream
          pages.rs
          stream.rs                 FeedEvent, pages, with_authors, new_posts, watch
        ports/repo.rs               + FeedSlice, feed_page, feed_item
        network/feed.rs             + feed_before, feed_post
        storage/mem/repo.rs         + feed_page, feed_item
        app/
          mod.rs                    + счётчик слушателей
          live.rs                   feed_stream, live, watch, watchers
          config.rs                 + feed_page, feed_lookups
    test-support/src/slow/posts.rs  + feed_page, feed_item
  tools/repl/
    Cargo.toml                      + futures
    src/session/mod.rs              + Action::Watch, Action::Unwatch
    src/terminal/mod.rs             + /watch, /unwatch, live
    src/run/mod.rs                  живые строки после ответа
  */tests/journey/m22_live_feed.rs  тесты курса в contracts, social-core и tools/repl
```

Остальное дерево не меняется.

## Приёмка

- `contracts/tests/journey/m22_live_feed.rs`: разбор `/watch` и `/unwatch`, ошибки, имена в `CHAT_COMMAND_NAMES`; новая версия `m11_events_notifiers.rs` проверяет начало списка;
- `social-core/tests/journey/m22_live_feed.rs`: страницы по курсору без повторов и пропусков, даже когда посты выходят посреди чтения; ошибка страницы приходит как `Err`; авторы подтягиваются параллельно, но не больше `feed_lookups` сразу (на замороженном времени); стрим ленив и не читает ленту впрок; `watch` даёт историю, `UpToDate` и живые посты только из ленты; пост, вышедший, пока грузилась первая страница, приходит ровно один раз; брошенный стрим отписывается, после `close` живая лента кончается;
- `tools/repl/tests/journey/m22_live_feed.rs`: автомат сессии, живые строки в терминале, `/unwatch` и `/logout` бросают стрим;
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву, стримы в `feed/stream.rs` (`impl Stream`, `unfold`, `buffered`, без `buffer_unordered`), фасад ленты в `app/live.rs`, свой `BoxFuture`, без `futures::executor`, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
printf 'register alice\nregister bob\nfollow bob alice\npost alice Утро\n/login bob\n/watch 5\npost alice Обед\npost alice Вечер\n/unwatch\npost alice Ночь\n' | cargo run -q -p repl
```

После `/watch 5` bob видит утренний пост, а обед и вечер появляются строками `вживую:` сами, без `feed`. Ночной пост выходит уже после `/unwatch`, и живой строки для него нет.
