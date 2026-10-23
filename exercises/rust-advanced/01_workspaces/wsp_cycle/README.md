# Круг в графе зависимостей

Соцсеть растёт: крейт `users` хранит пользователей, `posts` хранит посты. Пользователю нужны его посты
(для кармы), посту нужен пользователь (автор). Каждый крейт добавили через path-зависимость друг на
друга, и теперь cargo не открывает воркспейс вообще.

## Что должно получиться

Стрелки зависимостей идут в одну сторону:

```
app  -->  users  -->  contracts
 |                       ^
 +-----> posts  --------+
```

API, которое проверяют тесты (`crates/app/tests/api.rs`):

- `contracts::UserId(pub u64)`, сравнивается и копируется;
- `users::User { pub id: UserId, pub name: String }` и `User::new(id: u64, name: &str)`;
- `posts::Post { pub author: UserId, pub text: String }` и `Post::new(author: UserId, text: &str)`;
- `app::karma(user: &User, posts: &[Post]) -> u32`: десять очков за каждый пост пользователя;
- `app::report(user, posts) -> String` вида `ann: karma 10`.

`contracts` не зависит ни от каких крейтов. `users` и `posts` не знают друг о друге.

## Приёмка

```
cargo test --manifest-path acceptance/Cargo.toml
```

Тесты читают граф зависимостей из `cargo metadata`, а потом запускают `cargo test --workspace`
и `cargo run -p app`.
