# Веха 2. Сеть подписок

Один пользователь с постом это ещё не соцсеть, это дневник. Соцсеть начинается, когда люди находят друг друга по нику, подписываются и лайкают. Для этого нужно где-то хранить всех сразу и быстро искать: кто такой `alice`, на кого он подписан, в каких постах был `#rust`. Ровно для такого поиска по ключу и придуманы `HashMap`.

Заодно `src/lib.rs` пора разгрузить. Код расходится по модулям-папкам, а `lib.rs` становится фасадом: он только объявляет модули и выставляет наружу то, чем можно пользоваться. Тесты переезжают из `src/tests/` в `tests/` и видят крейт снаружи, как любой другой пользователь, поэтому всё, что им нужно, теперь `pub`.

## Модули в файлах

В главе про модули они жили внутри одного файла (`mod fruits { ... }`). Тот же модуль можно вынести в папку:

```rust
// src/lib.rs
mod domain;                      // Rust ищет src/domain/mod.rs
pub use domain::User;            // а это выставляет User наружу: backend::User

// src/domain/mod.rs
mod user;                        // а здесь ищет src/domain/user.rs
pub use user::User;

// src/domain/user.rs
pub struct User { /* ... */ }
```

`use crate::domain::User` внутри крейта и `use backend::User` в тестах ведут к одному и тому же типу. Соседний модуль доступен через `super::` или от корня через `crate::`.

## Что сделать

1. Разложи код по дереву ниже. В `src/lib.rs` остаются только `mod ...;` и `pub use ...;`, никаких типов. Типы из вехи 1 переезжают в `src/domain/` и становятся `pub` вместе с методами и полем внутри id: `pub struct UserId(pub u64);`. Строку `#![allow(dead_code)]` удали, глушить больше нечего.
2. Удали `src/tests/` и строку `mod tests;`: тесты курса теперь лежат в `tests/` и подключаются сами.
3. Добавь `Hash` в derive у `UserId` и `PostId`, иначе они не смогут быть ключами `HashMap`.
4. `domain/user.rs`: функция `pub fn normalize_name(name: &str) -> String` убирает пробелы по краям и приводит ник к нижнему регистру.
5. `domain/post.rs`: лайк без накруток. Пост помнит, кто его лайкнул (`Vec<UserId>` вместо счётчика):
   - `pub fn like(&mut self, by: UserId) -> bool`: `true`, если лайк от этого пользователя первый, иначе `false` и ничего не меняется;
   - `pub fn is_liked_by(&self, user: UserId) -> bool`;
   - `pub fn likes(&self) -> usize`: сколько разных людей лайкнули.
6. `network/mod.rs`: `pub struct Network` с пользователями в `HashMap<UserId, User>`, индексом ников `HashMap<String, UserId>` и постами в `Vec<Post>` в порядке публикации. Id выдаются по порядку, начиная с 1. Методы:
   - `pub fn new() -> Network`;
   - `pub fn register(&mut self, name: &str) -> UserId`: ник нормализуется; если такой ник уже есть, второго пользователя не появляется и возвращается id существующего;
   - `pub fn user_count(&self) -> usize`, `pub fn is_registered(&self, name: &str) -> bool` (ник тоже нормализуется);
   - `pub fn publish(&mut self, author: UserId, text: &str, visibility: Visibility) -> PostId` и `pub fn post_count(&self) -> usize`;
   - `pub fn like(&mut self, user: UserId, post: PostId) -> bool`: `false`, если поста нет или лайк повторный; `pub fn likes(&self, post: PostId) -> usize` (0 для несуществующего поста);
   - `pub fn follow(&mut self, who: UserId, whom: UserId) -> bool`, `pub fn is_following(&self, who: UserId, whom: UserId) -> bool`, `pub fn following_count(&self, user: UserId) -> usize`, `pub fn followers_count(&self, user: UserId) -> usize`;
   - `pub fn posts_with_tag(&self, tag: &str) -> Vec<PostId>`: посты с тегом в порядке публикации.
7. `network/follows.rs`: `Follows` с двумя картами `HashMap<UserId, Vec<UserId>>`: на кого подписан каждый и кто подписан на каждого. Повторная подписка ничего не меняет и даёт `false`. Подписка на себя тоже даёт `false` и не считается (честную ошибку для неё сделаем в следующей вехе).
8. `network/tags.rs`: `Tags` с индексом `HashMap<String, Vec<PostId>>`, `publish` кладёт туда теги поста.

Что считать хештегом: слово (кусок между пробелами), которое начинается с `#`. Тег это буквы, цифры и `_` сразу после решётки, до первого другого символа, в нижнем регистре. Из `#Rust,` получается `rust`, из `#tokio!` получается `tokio`, одинокая `#` тегом не считается, повтор тега в одном посте учитывается один раз. `posts_with_tag` принимает тег и с решёткой, и без, в любом регистре.

## Дерево после вехи

```
backend/
  Cargo.toml
  src/
    lib.rs                фасад: mod и pub use, без типов
    domain/
      mod.rs
      user.rs             UserId, User, normalize_name
      post.rs             PostId, Post, PostKind, Visibility
    network/
      mod.rs              Network
      follows.rs          Follows
      tags.rs             Tags
  tests/                  тесты курса, не правь
    m01_user_post.rs
    m02_follow_graph.rs
```

Файлов-свалок вроде `utils.rs` и `helpers.rs` не заводи: у каждой функции есть модуль, которому она принадлежит.

## Приёмка

- `tests/m01_user_post.rs`: тесты вехи 1, переехавшие наружу (лайк теперь с автором).
- `tests/m02_follow_graph.rs`: нормализация ника, один ник один пользователь, подписки в обе стороны без дублей и без подписки на себя, лайк от каждого один раз, поиск по хештегам.
- Архитектура: дерево как выше, `src/tests/` нет, в `src/` нет `allow(dead_code)`, `lib.rs` без типов, модули одного вида, нет `utils.rs` и `helpers.rs`.

## Как запустить

```
cd backend
cargo test
```
