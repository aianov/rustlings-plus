# Веха 4. Хранилище за трейтами

Пользователи и посты пока живут в памяти, и однажды это кончится: придут Postgres, Scylla, кэш в Redis. Если `Network` знает, что внутри у него `HashMap`, то переезд на базу перепишет половину соцсети. Поэтому договоримся заранее. `Network` будет знать только, что хранилище умеет: выдать id, сохранить, найти. Это обещание записывается трейтом, а память становится одной из его реализаций. Когда-нибудь рядом появится реализация на базе, и `Network` даже не заметит подмены.

Заодно соцсеть научится показывать себя людям: у профиля и поста появится текстовая карточка через `Display`.

## Что сделать

1. `src/ports/mod.rs`: два трейта. Сигнатуры ровно такие, обязательные методы без тела, остальные с реализацией по умолчанию через обязательные:

   ```rust
   pub trait UserStore {
       fn next_id(&mut self) -> UserId;
       fn insert(&mut self, user: User);
       fn get(&self, id: UserId) -> Option<&User>;
       fn id_by_name(&self, name: &str) -> Option<UserId>;
       fn count(&self) -> usize;

       fn contains(&self, id: UserId) -> bool { ... }
       fn is_taken(&self, name: &str) -> bool { ... }
       fn find_by_name(&self, name: &str) -> Option<&User> { ... }
   }

   pub trait PostStore {
       fn next_id(&mut self) -> PostId;
       fn insert(&mut self, post: Post);
       fn get(&self, id: PostId) -> Option<&Post>;
       fn get_mut(&mut self, id: PostId) -> Option<&mut Post>;
       fn all(&self) -> &[Post];

       fn count(&self) -> usize { ... }
       fn contains(&self, id: PostId) -> bool { ... }
       fn last_by(&self, author: UserId) -> Option<&Post> { ... }
   }
   ```

   Хранилище ничего не нормализует и не проверяет: ник приходит к нему уже готовым. Правила остаются в домене и в `Network`. `all()` отдаёт посты в порядке публикации, `last_by` самый свежий пост автора.
2. `src/storage/`: `MemUserStore` (`users.rs`) и `MemPostStore` (`posts.rs`), реализации трейтов в памяти. У каждой `pub fn new()`, id выдаются по порядку с 1. Внутри что хочешь: например, `HashMap` пользователей плюс индекс ников, `Vec` постов плюс карта «id поста → позиция в Vec».
3. `Network` становится `pub struct Network<U, P>` с полями-хранилищами `users: U` и `posts: P` (подписки и теги остаются в `network/`, это не хранилище, а правила сети). Конструкторы:
   - `pub fn with_stores(users: U, posts: P) -> Network<U, P>` для любых `U: UserStore, P: PostStore`;
   - `pub fn new() -> Network<U, P>`, когда оба хранилища умеют `Default`, и `impl Default` для того же случая.
   Все операции и поиск из прошлых вех работают как раньше, но через методы трейтов. id пользователя и поста выдаёт хранилище (`next_id`).
4. В `src/lib.rs` выставь наружу трейты и хранилища, а для удобства заведи псевдоним `pub type MemNetwork = Network<MemUserStore, MemPostStore>;`. `MemNetwork::new()` даёт сеть в памяти. Тесты прошлых вех переписаны с `Network` на `MemNetwork`, больше в них ничего не поменялось.
5. Модули `domain/` и `network/` не знают про `storage`: в их коде нет ни `storage::`, ни `Vec<Post>`. Хранилищем владеет тот, кто собирает сеть.
6. Карточки через `std::fmt::Display`:

   | что                     | пример                            |
   |-------------------------|-----------------------------------|
   | `User` без био          | `@alice`                          |
   | `User` с био            | `@alice: Пишу на Rust`            |
   | текстовый `Post`        | `[7] Привет, мир! (лайков: 2)`    |
   | репост                  | `[8] репост [7] (лайков: 0)`      |

   В квадратных скобках число из id.

Кто про кого знает после вехи:

```
domain     ни про кого
ports      domain
storage    ports, domain
network    ports, domain
lib.rs     про всех: собирает и выставляет наружу
```

## Дерево после вехи

```
backend/
  Cargo.toml
  src/
    lib.rs                фасад + type MemNetwork
    domain/
      mod.rs  error.rs
      user.rs             + impl Display for User
      post.rs             + impl Display for Post
    network/
      mod.rs              Network<U, P>
      follows.rs  tags.rs
    ports/
      mod.rs              UserStore, PostStore
    storage/
      mod.rs
      users.rs            MemUserStore
      posts.rs            MemPostStore
  tests/                  тесты курса, не правь
    m01_user_post.rs
    m02_follow_graph.rs   теперь через MemNetwork
    m03_domain_errors.rs  теперь через MemNetwork
    m04_store_traits.rs
```

## Приёмка

- `tests/m02_follow_graph.rs` и `tests/m03_domain_errors.rs`: всё прежнее поведение на `MemNetwork`.
- `tests/m04_store_traits.rs`: `Network` работает поверх чужого хранилища из теста (оно реализует только обязательные методы, так что методы по умолчанию обязаны быть в трейте), id выдаёт хранилище, методы по умолчанию работают на `MemUserStore` и `MemPostStore`, карточки печатаются ровно как в таблице.
- Архитектура: всё из прошлых вех, плюс есть `src/ports/mod.rs` и `src/storage/mod.rs`, в `src/domain/` и `src/network/` нет `storage::`, в `src/network/` нет `Vec<Post>`.

## Как запустить

```
cd backend
cargo test
```
