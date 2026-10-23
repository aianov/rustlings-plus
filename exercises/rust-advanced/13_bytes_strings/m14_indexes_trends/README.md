# Веха 14. Индексы, ветки и тренды

Постов стало много, и соцсеть начала задыхаться на простых вопросах. Чтобы показать профиль alice, она перебирает все посты сети. Удалённый пост навсегда остаётся в индексе тегов, и из поиска его прячет только фильтр на выходе. Спорить про Rust приходится в личке, потому что комментариев нет. А вчера кто-то зарегистрировал «аlice» с русской «а», и полсотни человек подписались не на ту alice.

Пора навести порядок в данных. Посты автора получат упорядоченный индекс и страницы по курсору, теги станут множествами, удаление будет чистить все индексы разом. Под постами вырастут ветки комментариев, а `trends` покажет, что обсуждают за сутки. Длинный пост в ленте превратится в аккуратное превью, которое не режет букву пополам, а ник станет строго ASCII.

## Что сделать

1. **contracts, `username/`: ник только из ASCII.**
   - `normalize_name` обрезает пробелы по краям (как раньше) и переводит в нижний регистр только ASCII: `to_ascii_lowercase`. Если заглавных ASCII-букв нет, копии нет (`Cow::Borrowed`). Остальные символы не трогаются: `"ÄLİCE"` становится `"Älİce"`, и длина в байтах не меняется.
   - `Username::try_from` проверяет нормализованный ник по порядку: пустой это `NameError::Empty`; первый символ не из `a-z`, `0-9` и `_` это `NameError::BadChar(символ)` (символ как он есть после нормализации: у `"ÄLICE"` это `'Ä'`); длиннее `MAX_NAME_LEN` это `NameError::TooLong`. Ник теперь ASCII, поэтому длина в байтах равна длине в символах.
   - `NameError::BadChar(char)` новый вариант, `Display`: `в нике можно только a-z, 0-9 и _, а 'я' нельзя` (символ печатается через `{:?}`). Ник с провода (`users.register`) проходит те же проверки, причина в `BadData` это этот текст.

2. **contracts, `ids/`.** `UserId`, `PostId` и новый `CommentId(pub u64)` сравниваются по числу (`PartialOrd`, `Ord`). `CommentId` устроен как остальные id: `Display` даёт `c_5`, `FromStr` разбирает его обратно (`нужно c_<число>` в тексте `IdError`), serde пишет и читает строку.

3. **contracts, `cursor/`: курсор страницы.** `pub type Timestamp = u64` (секунды с начала эпохи UNIX) и `Cursor { pub time: Timestamp, pub post: PostId }` с `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`. Курсоры сравниваются сначала по времени, при равном времени по id: больший id новее. `Display` даёт `1700000000:42`. `FromStr` (`Err = ParseError`) принимает ровно `<цифры>:<цифры>`, оба числа помещаются в `u64`; всё остальное (`"100"`, `"100:"`, `"+100:5"`, `"100:5:6"`, `"a:b"`) это `ParseError::BadCursor(текст)` с `Display` `неверный курсор "100": нужно <время>:<id поста>`.

4. **contracts, `command/`: пять команд.** `COMMAND_NAMES` дополняется в конце именами `posts`, `comment`, `reply`, `thread`, `trends` (всего 14). `размер` и `N` по умолчанию `DEFAULT_FEED_LIMIT`, числа разбираются как раньше (`BadNumber`), лишнее слово это `ExtraArgument`.

   | строка | `Command` | чего не хватает (`MissingArgument`) |
   |---|---|---|
   | `posts <кто> <автор> [размер [курсор]]` | `Posts { who, author, size, after: Option<Cursor> }` | `ник`, `чьи посты` |
   | `comment <ник> <id поста> <текст>` | `Comment { who, post: PostId, text }` | `ник`, `id поста`, `текст комментария` |
   | `reply <ник> <id комментария> <текст>` | `Reply { who, to: CommentId, text }` | `ник`, `id комментария`, `текст ответа` |
   | `thread <ник> <id поста>` | `Thread { who, post: PostId }` | `ник`, `id поста` |
   | `trends [N]` | `Trends { limit }` | |

   Текст комментария и ответа это весь хвост строки, как у `post`. Конверты и методы JSON из вехи 13 не меняются: новые возможности пока только в текстовых командах.

5. **social-core, `domain/`: время, превью и новые ошибки.**
   - `domain/clock.rs`: `now() -> Timestamp` читает `SystemTime`. Это единственное место ядра, которое смотрит на часы; всё остальное получает время аргументом.
   - У поста есть время: `Post::new` не меняется (время 0), `Post::at(self, time) -> Post` задаёт его, `time()` его отдаёт, `cursor()` отдаёт `Cursor { time, post: id }`. Лайки хранятся множеством (`HashSet<UserId>`), повторный лайк по-прежнему `false`. `Post::delete` становится `pub(crate)`: удалить пост можно только через `Network::delete_post`.
   - `PREVIEW_LEN: usize = 80` и `preview(text: &str, max_bytes: usize) -> Cow<'_, str>`: текст не длиннее `max_bytes` байт отдаётся как есть, без копии. Длинный режется по ближайшей границе символа не дальше `max_bytes`, пробелы в конце куска срезаются, и дописывается `…`: `preview("привет", 5)` это `"пр…"`. `Post::preview(&self)` это `preview(text, PREVIEW_LEN)`. Карточка поста (`Display`) показывает превью: `[3] <превью> (лайков: 0)`, сам `text()` не обрезается.
   - `DomainError`: `BadNameChar(char)` (из `NameError::BadChar`, текст тот же), `UnknownComment(CommentId)` (`нет комментария с id 3`), `EmptyComment` (`комментарий не может быть пустым`), `CommentTooLong` (`комментарий длиннее 280 символов`). `social_core::Error::code()` для `UnknownComment` даёт `not_found`.

6. **social-core, `network/`: индексы.**
   - `publish_at(author, text, visibility, at: Timestamp) -> Result<PostId, DomainError>` публикует пост со временем `at`, а `publish` это `publish_at(.., now())`. Так же `like_at(user, post, at) -> Result<bool, DomainError>` и `like` с `now()`.
   - Индекс автора (`network/authors.rs`): у каждого автора упорядоченное множество курсоров его живых постов (`BTreeSet<Cursor>` или `BTreeMap` с ключом `Cursor`).
   - `author_page(viewer, author, after: Option<Cursor>, size) -> Result<AuthorPage<'_>, DomainError>`, где `AuthorPage<'a> { pub posts: Vec<&'a Post>, pub next: Option<Cursor> }`. На странице до `size` постов автора, которые `viewer` может видеть (`can_view`), от новых к старым по курсору. `after` это курсор последнего поста прошлой страницы, сам он на новую страницу не попадает. `next` это курсор последнего поста страницы, если за ним есть ещё видимые посты, иначе `None` (и у пустой страницы, и при `size` 0). Mute на страницу автора не влияет: это профиль, а не лента. Незнакомый `viewer` или `author` это `UnknownUser`. Страница читает индекс через `range` от курсора, а не перебирает все посты.
   - Индекс тегов (`network/tags.rs`): тег это множество id (`BTreeSet<PostId>` или `HashSet<PostId>`). `posts_with_tag` отдаёт id по возрастанию, `search_tag` как раньше, от новых к старым.
   - `delete_post` по-прежнему мягкий (пост остаётся в хранилище с пометкой, повторное удаление `Ok(false)`), но теперь вынимает пост из индекса автора, из тегов, из трендов и удаляет его комментарии. Пустые записи индексов (автор без постов, тег без постов) исчезают.
   - Лента (`feed`, `page`, `search_tag`) идёт в порядке публикации, как раньше; время поста задаёт порядок на странице автора.

7. **social-core, `trends/`.** `Trend { pub post: PostId, pub likes: usize }` (`Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`), `TRENDS_WINDOW: u64 = 86_400` (сутки) и `Network::trending(now, window, n) -> Vec<Trend>`. В окно входят лайки со временем `t`, где `now - window < t <= now` (вычитание не уходит ниже нуля). В тренды попадают только публичные посты, у которых в окне есть лайки; удалённые посты уходят. Порядок: по убыванию лайков, при равенстве раньше меньший id. Не больше `n`, при `n` 0 пусто. Кандидатов держит `BinaryHeap` размера не больше `n` (min-куча через `Reverse`), сортировать все посты нельзя.

8. **social-core, `comments/`: ветки.** `Comment` с методами `id()`, `post()`, `author()`, `parent() -> Option<CommentId>` и `text()`, `MAX_COMMENT_LEN: usize = 280`. Хранилище плоское: комментарии по id и индекс «пост → id его комментариев по порядку», вложенности внутри `Comment` нет. Id комментариев общие на всю сеть, с единицы.
   - `Network::comment(author, post, text) -> Result<CommentId, DomainError>` и `Network::reply(author, parent: CommentId, text)`: ответ попадает в пост родителя. Проверки по порядку: автор существует (`UnknownUser`); у ответа родитель существует (`UnknownComment`); пост есть, не удалён и автор комментария может его видеть (иначе `UnknownPost`: чужой личный пост для тебя не существует); текст без пробелов по краям не пустой (`EmptyComment`) и не длиннее `MAX_COMMENT_LEN` символов (`CommentTooLong`).
   - `Network::find_comment(id) -> Option<&Comment>`.
   - `Network::thread(viewer, post) -> Result<Vec<(usize, &Comment)>, DomainError>`: читатель существует, пост виден ему (иначе `UnknownPost`). Ветка идёт обходом в глубину: комментарии верхнего уровня по порядку, за каждым его ответы (и ответы на ответы), у каждого глубина, у верхнего уровня 0.

9. **social-core, `shared/` и `commands/`.** `SharedNetwork::comment` и `SharedNetwork::reply` повторяют методы сети. Команды отвечают так (строка поста как в ленте: `@alice [3] <превью> (лайков: 0)`):

   | команда | ответ | если пусто |
   |---|---|---|
   | `posts bob alice 2` | строки постов страницы, потом `дальше: <курсор>`, если есть ещё | `постов нет` |
   | `comment bob 3 Класс` | `комментарий [1] к посту [3]` | |
   | `reply alice 1 Спасибо` | `ответ [2] на комментарий [1]` | |
   | `thread bob 3` | `[1] @bob: Класс`, ответы с отступом в два пробела на уровень: `  [2] @alice: Спасибо` | `комментариев пока нет` |
   | `trends 5` | `@alice [3] три (лайков: 5), за сутки +2` за `TRENDS_WINDOW` от текущего времени | `трендов пока нет` |

   Терминал узнаёт новые команды сам: шина регистрирует всё из `COMMAND_NAMES`.

10. **Старые тесты.** Веха кладёт новые версии двух тестов прошлых вех: `social-core/tests/m03_domain_errors.rs` (ник из кириллицы теперь `BadNameChar`) и `contracts/tests/m11_events_notifiers.rs` (в `COMMAND_NAMES` 14 команд).

## Дерево после вехи

```
backend/
  src/crates/
    contracts/
      src/
        lib.rs
        cursor/mod.rs               Timestamp, Cursor
        ids/mod.rs                  + Ord, CommentId
        username/mod.rs             ASCII-ник, NameError::BadChar
        command/                    + posts, comment, reply, thread, trends; ParseError::BadCursor
        api/  chat/  envelope/  error/  input/  push/
      tests/
        m14_indexes_trends.rs       тесты курса
        m11_events_notifiers.rs     новая версия: 14 команд
    social-core/
      src/
        lib.rs
        comments/mod.rs             Comment, MAX_COMMENT_LEN, плоское хранилище, ветка
        trends/mod.rs               Trend, TRENDS_WINDOW, топ-N на min-куче
        domain/
          clock.rs                  now(): единственное чтение часов в ядре
          post.rs                   время поста, курсор, лайки множеством, preview
          error.rs  mod.rs  user.rs
        network/
          mod.rs                    Network: пользователи, подписки, скрытие, переписка, события
          posts.rs                  publish_at, like_at, delete_post, author_page, trending
          authors.rs                индекс автора по Cursor, AuthorPage
          tags.rs                   тег: множество id
          comments.rs               comment, reply, find_comment, thread
          feed.rs  follows.rs  mutes.rs
        shared/mod.rs               + comment, reply
        commands/                   + пять команд
        error/mod.rs                UnknownComment это not_found
        api/  chat/  events/  feed/  notify/  ports/  storage/
      tests/
        m14_indexes_trends.rs       тесты курса
        m03_domain_errors.rs        новая версия: кириллица в нике отвергается
  tools/repl/
    tests/
      m14_indexes_trends.rs         тесты курса
```

Остальное дерево не меняется.

## Приёмка

- `src/crates/contracts/tests/m14_indexes_trends.rs`: ASCII-ник, порядок проверок и первый неподходящий символ, `normalize_name` без смены длины, ник с провода; порядок id, `CommentId` на проводе; порядок, текст и отказы курсора; разбор пяти команд и их ошибки;
- `src/crates/social-core/tests/m14_indexes_trends.rs`: страницы автора по времени, одинаковое время без повторов и потерь на стыках, страница ровно впритык без пустого хвоста, курсор после удаления поста на границе, видимость и mute, пустые и кривые запросы; удаление чистит все индексы; окно трендов с обеих сторон, порядок при равенстве, топ-5 из 40 постов, только публичные; ветка в глубину, правила комментариев и видимость; превью по границе символа и карточка поста; ник в сети; текстовые команды;
- `tools/repl/tests/m14_indexes_trends.rs`: комментарии, ветка и тренды из терминала, страница автора и превью, понятный отказ для ника на кириллице;
- остальные тесты прошлых вех не меняются и проходят;
- `cargo make check` зелёный.

Архитектура: новые файлы на месте, каждый файл в `src/crates/*/src` не длиннее 300 строк, индекс автора упорядоченный и без `..=`, теги множествами, тренды на куче с `Reverse` и без сортировки, `Post::delete` не публичный, часы только в `domain/clock.rs`, ник без юникодных `to_lowercase` и `is_alphabetic`, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
printf 'register alice\nregister bob\nregister аlice\nfollow bob alice\npost alice Что почитать про #rust?\npost alice Второй пост\ncomment bob 1 Растбук\nreply alice 1 Спасибо!\nlike bob 1\nthread bob 1\nposts bob alice 1\ntrends\n' | cargo run -q -p repl
```

Последняя строка `posts` это курсор: допиши его к команде (`posts bob alice 1 <курсор>`), и получишь следующую страницу.
