# Веха 12. Лента страницами

Лента до сих пор собирает `Vec` на каждый запрос: даже если человеку нужен один свежий пост, сеть проходит всех, фильтрует и складывает в вектор. Пока постов сотня, это незаметно. Когда их станет сто тысяч, а терминал покажет десять, это будет главной тратой памяти. Заодно пользователи просят две вещи: спрятать надоевшего автора, не отписываясь от него, и удалить свой неудачный пост. Обе превращаются в фильтры ленты, и как раз хорошо, что фильтр в ленивой цепочке ничего не стоит, пока по ней никто не идёт.

Поэтому лента становится ленивым итератором, который собирает только тот, кому нужен результат, а листать её помогает свой адаптер `.pages(n)`, тот самый из главы.

## Что сделать

1. **social-core, `feed/`: адаптер страниц.** `feed/pages.rs`: структура `Pages<I: Iterator>` с `Item = Vec<I::Item>` и трейт-расширение `PagesExt` с методом `pages(self, size: usize) -> Pages<Self>` и общей реализацией для любого `I: Iterator`. Экспорт `Pages` и `PagesExt` из корня крейта.
   - страница это следующие `size` элементов; последняя может быть неполной, пустой страницы не бывает;
   - адаптер ленивый: за страницу из источника читается ровно страница;
   - после первого `None` источник больше не опрашивается (адаптер держит его через `fuse`);
   - `pages(0)` не паникует, а просто не даёт страниц: ядро не паникует ни на каком вводе;
   - `nth(n)` переопределён: пропущенные страницы не собираются, у источника пропускается `n * size` элементов, а переполнение умножения значит «такой страницы нет»;
   - в `feed/` нет ни `collect`, ни `assert!`; страницу наполняет `extend`. Не резервируй память по `size`: он приходит от пользователя.
2. **social-core: лента и поиск отдают итераторы.**
   - `Network::feed(&self, viewer) -> impl Iterator<Item = &Post> + '_`: свои посты и посты подписок, новые сверху, с правилами видимости, без скрытых авторов и без удалённых постов. Лимита больше нет: сколько взять, решает вызывающий (`.take(n)`, `.pages(n)`);
   - `Network::search_tag(&self, viewer, tag) -> impl Iterator<Item = &Post> + '_` с теми же фильтрами;
   - `post_count_of` и `likes_received` не считают удалённые посты;
   - в `network/feed.rs` ни одна функция не возвращает `Vec`, твои тесты рядом с лентой остаются (`#[cfg(test)]`).
3. **social-core: скрытые авторы.** `network/mutes.rs` хранит, кого кто скрыл. `Network::mute(who, whom) -> Result<bool, DomainError>` (`true`, если скрыл впервые; оба существуют, иначе `UnknownUser`; себя нельзя: новый `DomainError::SelfMute`, `нельзя скрыть самого себя`) и `Network::is_muted(who, whom) -> bool`. Скрытие не отменяет подписку и касается только того, кто скрыл.
4. **social-core: мягкое удаление.** `Post::is_deleted()` и `Post::delete(&mut self) -> bool`. `Network::delete_post(who, post) -> Result<bool, DomainError>`: пользователь существует (`UnknownUser`), пост существует (`UnknownPost`), удаляет только автор (новый `DomainError::NotAuthor(PostId)`, `удалить пост [3] может только автор`); `true`, если удалён сейчас, `false`, если уже был удалён. Пост остаётся в хранилище и виден через `post(id)`, но пропадает из ленты и поиска, а лайк удалённого поста даёт `UnknownPost`. `SharedNetwork` получает `mute` и `delete_post`.
5. **contracts и команды.** В `Command` три варианта, в `COMMAND_NAMES` три имени (в конце, в этом порядке):

   | строка | вариант | ответ |
   |---|---|---|
   | `mute <ник> <кого скрыть>` | `Mute { who, whom }` | `@bob больше не видит @alice в ленте` или `@bob уже скрыл @alice` |
   | `delete <ник> <id поста>` | `Delete { who, post }` | `пост [3] удалён` или `пост [3] уже удалён` |
   | `page <ник> <номер> [размер]` | `Page { who, number, size }`, размер по умолчанию `DEFAULT_FEED_LIMIT` | строки ленты как у `feed` или `страницы 3 нет` |

   Ошибки разбора как у соседей: `MissingArgument("кого скрыть")`, `MissingArgument("id поста")`, `MissingArgument("номер страницы")`, `BadNumber`, `ExtraArgument`. Страницы нумеруются с 1, страница 0 это `страницы 0 нет`. `feed <ник> [сколько]` работает как раньше, но через `.take(сколько)`. Терминалу ничего делать не нужно: шина регистрирует сетевые команды по `COMMAND_NAMES`.

## Дерево после вехи

```
backend/
  src/crates/
    contracts/
      src/command/mod.rs          Command (+ Mute, Delete, Page), COMMAND_NAMES из 9 имён
      tests/
        m11_events_notifiers.rs   новая версия от этой вехи
        m12_feed_pages.rs         тесты курса
    social-core/
      src/
        lib.rs
        feed/
          mod.rs                  mod pages и pub use
          pages.rs                Pages, PagesExt
        network/
          mod.rs                  mute, delete_post
          feed.rs                 feed, search_tag на итераторах, твои тесты
          follows.rs  tags.rs
          mutes.rs                кто кого скрыл
        domain/  commands/  events/  notify/  shared/  chat/  ports/  storage/
      tests/
        m05_feed.rs               новая версия от этой вехи
        m06_shared_threads.rs     новая версия от этой вехи
        m12_feed_pages.rs         тесты курса
  tools/repl/tests/
    m12_feed_pages.rs             тесты курса
```

Остальное дерево не меняется.

## Приёмка

- `src/crates/social-core/tests/m12_feed_pages.rs`: страницы у любого итератора, ленивость (счётчик прочитанного), `nth` без лишнего чтения и без переполнения, `fuse` после конца; лента как итератор со ссылками в хранилище; скрытие и удаление со всеми ошибками; команды `feed`, `page`, `mute`, `delete` через `execute_line`, включая огромные номер и размер страницы;
- `src/crates/contracts/tests/m12_feed_pages.rs`: разбор трёх команд и их ошибки;
- `tools/repl/tests/m12_feed_pages.rs`: новые команды доступны в терминале со статусами ответов;
- `m05_feed.rs`, `m06_shared_threads.rs` и `m11_events_notifiers.rs` (contracts) приходят в новой версии под новый API;
- `cargo make check` зелёный.

Архитектура: новые файлы на месте, в `feed/` нет `collect` и `assert!`, `PagesExt` это трейт, `network/feed.rs` отдаёт `impl Iterator<Item = &Post>` и не возвращает `Vec`, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
printf 'register alice\nregister bob\nfollow bob alice\npost alice раз\npost alice два\npost alice три\npage bob 1 2\npage bob 2 2\ndelete alice 3\nmute bob alice\nfeed bob\n' | cargo run -q -p repl
```
