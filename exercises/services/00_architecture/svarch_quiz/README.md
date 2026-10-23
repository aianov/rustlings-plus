# post-service целиком

Прототип из прошлой задачи вырос: посты должны публиковать только существующие пользователи, а
существуют они в соседнем сервисе. К тому же в продакшене один и тот же пост от нетерпеливого
пользователя прилетает дважды за доли секунды (двойной клик), и нам нужно ровно одно сообщение.
Собери `post-service` по слоям с нуля. Здесь нет прототипа: только контракты и приёмочные тесты.

## Что уже есть

- Воркспейс и крейты `contracts` (`UserId`, `PostId`) и `store-kit` (`Store<T>`: данные под замком, доступ только через `with`).
- `src/microservices/post-service/Cargo.toml` с зависимостями (`serde`, `serde_json`, `futures`).
- Приёмочные тесты в `tests/acceptance.rs`. Их нельзя править.
- Пустой `src/lib.rs`.

## Раскладка

```
src/microservices/post-service/src/
├── main.rs              до 20 строк
├── lib.rs
├── bootstrap/mod.rs
├── common/mod.rs        доменный тип Post
├── controller/mod.rs    Request, Response, Controller
├── dto/mod.rs
├── service/mod.rs       PostService, PostError, порт UserDirectory
└── repository/          mod.rs (трейт) и memory.rs (реализация)
```

## Контракт, который знают тесты

В `post_service::service`:

```rust
pub enum DirectoryError { Unavailable }          // с Debug

pub trait UserDirectory: Send + Sync {
    fn exists(&self, id: UserId) -> BoxFuture<'_, Result<bool, DirectoryError>>;
}
```

В `post_service::controller`: `Request { method, path, body }` и `Response { status, body }` (все поля
`pub`, строки `String`), `Controller` с методом `pub async fn handle(&self, req: Request) -> Response`.

В `post_service::common`: `Post { id: PostId, author: UserId, text: String }` с `pub` полями.

В `post_service::repository`:

```rust
pub enum RepoError { Conflict, Unavailable }          // с Debug

pub trait PostRepository: Send + Sync {
    fn create_if_absent(&self, author: UserId, text: String) -> BoxFuture<'_, Result<Post, RepoError>>;
    fn find(&self, id: PostId) -> BoxFuture<'_, Option<Post>>;
    fn by_author(&self, author: UserId) -> BoxFuture<'_, Vec<Post>>;
}

pub struct InMemoryPosts;      // Default и new(store: Store<Vec<Post>>)
```

`InMemoryPosts` держит данные только в `Store<Vec<Post>>` и достаёт их через `with`. Тест подставляет
`Store::with_hook(...)`: хук срабатывает после каждого `with`.

В `post_service::bootstrap`: `pub fn build(users: Arc<dyn UserDirectory>) -> Controller` и
`pub fn build_with(users: Arc<dyn UserDirectory>, posts: Arc<dyn PostRepository>) -> Controller`.

## Что должен уметь сервис

1. `POST /posts` с телом `{"author": N, "text": "..."}`. Текст обрезается по краям.
   - 201 и `{"id", "author", "text"}`;
   - 400 `bad_json` на битое тело;
   - 422 `empty_text` на пустой текст, 422 `text_too_long` на текст длиннее 280 символов (не байт);
   - 404 `author_not_found`, если сосед ответил «такого нет»;
   - 503 `users_unavailable`, если сосед не ответил (это не «автора нет»);
   - 409 `duplicate_post`, если у этого автора уже есть пост с таким же текстом (после обрезки);
   - 503 `storage_unavailable`, если репозиторий ответил `RepoError::Unavailable`.
2. `GET /posts/{id}`: 200, 400 `bad_id`, 404 `post_not_found`.
3. `GET /users/{id}/posts`: 200 и массив постов автора в порядке создания (для незнакомого автора пустой).
4. Остальные пути: 404 `not_found`.
5. Ошибки приходят телом `{"error": "<код>"}`.

## Правила архитектуры

- Дешёвые проверки (JSON, пустой текст, длина) идут раньше, чем запрос к соседу.
- Правила только в `service`. В нём нет `serde_json`, `dto::`, `controller::`, слов `Request` и `Response`.
- В `controller` нет `repository::`. В `repository` нет `service::`, `controller::`, `dto::`, `serde`.
- Отказ не тратит номер поста: id выдаётся только при успешной вставке.
- Дубликат отклоняет репозиторий, а не service: тест одновременно шлёт четыре одинаковых запроса,
  останавливая каждый на хуке `Store` после обращения к данным, и ждёт ровно один 201.
- Сборка зависимостей только в `bootstrap::build`.

## Критерии приёмки

`cargo test --workspace` проходит, `cargo clippy --workspace --all-targets -- -D warnings` чист.
Тесты проверяют раскладку и стрелки, все пути API, перевод ошибки соседа, порядок проверок, номера
постов под нагрузкой и гонку дубликатов (пять раундов по четыре запроса), перевод ошибки хранилища.

## Подсказки по теории

README главы: «Репозиторий как трейт», «async-порты», «Атомарность», «Чужие данные». Порт к соседу
объявляется в `service`, потому что это service его потребитель.
