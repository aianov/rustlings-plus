# Раскладка post-service

Постов в соцсети стало много, и под них выделили отдельный сервис. Пока что он живёт одним файлом:
рабочий прототип лежит в `src/microservices/post-service/src/main.rs` и умеет создавать пост и читать
его по номеру. Работает, но следующий разработчик не найдёт в нём ни одного правила. Твоя задача:
не поменять поведение, а разложить код по слоям так, как принято в курсе.

## Что уже есть

- Воркспейс с крейтом `contracts` (`UserId`, `PostId`): его знают все сервисы.
- Крейт `post-service` с прототипом в `main.rs`.
- Приёмочные тесты в `tests/acceptance.rs`. Их нельзя править (CLI восстановит файл).

## Что сделать

1. Добавь `src/lib.rs` с модулями `bootstrap`, `common`, `controller`, `dto`, `repository`, `service`.
2. Каждый слой это папка с `mod.rs`: `src/<слой>/mod.rs`. Рядом с папкой не должно лежать `<слой>.rs`.
3. Разнеси код:
   - `common`: доменный тип `Post`;
   - `repository`: трейт `PostRepository` (`insert`, `find`) и `InMemoryPosts` на `Mutex`, реализация в
     отдельном файле `repository/memory.rs`;
   - `service`: `PostService` с правилами (пустой текст, 280 символов) и `PostError`. Он принимает
     доменные значения и не знает слов «JSON» и «422»;
   - `dto`: `CreatePostDto`, `PostDto` и `From<Post> for PostDto`;
   - `controller`: `Request { method, path, body }` и `Response { status, body }` (поля `pub`, строки
     `String`), `Controller::handle(&self, req: Request) -> Response`, разбор пути и перевод ошибок в статусы;
   - `bootstrap`: `build() -> Controller`, единственное место, где создаётся репозиторий.
4. `main.rs` должен остаться тонким (до 20 строк): вызвать `post_service::bootstrap::build()` и один
   раз показать ответ. Хранилища в нём быть не должно.

Дерево в итоге:

```
src/microservices/post-service/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── bootstrap/mod.rs
│   ├── common/mod.rs
│   ├── controller/mod.rs
│   ├── dto/mod.rs
│   ├── service/mod.rs
│   └── repository/
│       ├── mod.rs
│       └── memory.rs
└── tests/acceptance.rs
```

## Критерии приёмки

`cargo test --workspace` проходит. Тесты проверяют:

- раскладку: файлы и папки на своих местах, нет `utils.rs` и `helpers.rs`;
- стрелки: в `controller` нет `repository::`; в `service` нет `serde_json`, `dto::`, `controller::`,
  `Request`, `Response`; в `repository` нет `service::`, `dto::`, `serde`;
- поведение API: `POST /posts` (201, 400 на битый JSON, 422 на пустой или слишком длинный текст),
  `GET /posts/{id}` (200, 400, 404), остальные пути 404;
- две сборки `bootstrap::build()` не делят хранилище.

Запуск: `cargo test --workspace` в корне проекта.

## Подсказки по теории

Читай README главы: «Пять слоёв», «Куда смотрят стрелки», «Composition root». Начинай снизу: сначала
`common` и `repository`, потом `service`, потом `dto` и `controller`, в конце `bootstrap`.
