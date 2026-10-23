# Веха 3. Ошибки соцсети

Первые пользователи уже нашли, как всё сломать. Кто-то зарегистрировался с ником из одних пробелов, кто-то подписался сам на себя, а кто-то лайкнул пост номер 999, которого никогда не было. Сеть молча отвечала `false`, и никто не понимал, что пошло не так. Хватит молчать: у каждого правила соцсети теперь есть имя, а у каждой операции честный ответ «получилось» или «вот что не так».

Поиск тоже перестаёт врать. «Пользователь по нику» и «последний пост автора» могут ничего не найти, и это нормальный ответ, а не ошибка. Для такого есть `Option`.

## Что сделать

1. `src/domain/error.rs`: `pub enum DomainError` с derive `Debug, Clone, PartialEq, Eq` и вариантами из таблицы ниже. Реализуй для него `std::fmt::Display` (ровно эти фразы) и `std::error::Error`.
2. Константы `pub const MAX_NAME_LEN: usize = 20;` в `domain/user.rs` и `pub const MAX_POST_LEN: usize = 280;` в `domain/post.rs`. Длина считается в символах (`chars().count()`), а не в байтах: «я» занимает два байта, но это одна буква.
3. Проверки правил живут в домене, рядом с типами:
   - `validate_name(name: &str) -> Result<String, DomainError>` в `domain/user.rs`: нормализует ник и проверяет, что он не пустой и не длиннее `MAX_NAME_LEN`;
   - `validate_text(text: &str) -> Result<String, DomainError>` в `domain/post.rs`: обрезает пробелы по краям и проверяет, что пост не пустой и не длиннее `MAX_POST_LEN`.
4. Операции `Network` возвращают `Result` и пробрасывают ошибки через `?`:
   - `register(&mut self, name: &str) -> Result<UserId, DomainError>`: ошибка ника или `NameTaken`, если такой ник уже есть;
   - `publish(&mut self, author: UserId, text: &str, visibility: Visibility) -> Result<PostId, DomainError>`: `UnknownUser`, если автора нет, потом ошибки текста; текст сохраняется обрезанным;
   - `like(&mut self, user: UserId, post: PostId) -> Result<bool, DomainError>`: `UnknownUser`, потом `UnknownPost`; `Ok(false)` для повторного лайка;
   - `follow(&mut self, who: UserId, whom: UserId) -> Result<bool, DomainError>`: сначала оба пользователя должны существовать (`UnknownUser`), потом `SelfFollow`; `Ok(false)` для повторной подписки.
   После ошибки сеть не меняется: ни пользователя, ни поста, ни лайка, ни подписки.
5. Поиск отдаёт `Option`:
   - `user(&self, id: UserId) -> Option<&User>`;
   - `find_user(&self, name: &str) -> Option<&User>`: ник нормализуется;
   - `post(&self, id: PostId) -> Option<&Post>`;
   - `last_post(&self, author: UserId) -> Option<&Post>`: самый свежий пост этого автора.
6. `src/lib.rs` выставляет наружу ещё `DomainError`, `MAX_NAME_LEN`, `MAX_POST_LEN`.
7. В коде крейта больше нет `unwrap()` и `panic!`. Если что-то может не получиться, это `Result` или `Option`, и решает вызывающий.

| вариант                 | когда                                   | Display                               |
|-------------------------|-----------------------------------------|---------------------------------------|
| `EmptyName`             | ник пустой после нормализации           | `ник не может быть пустым`            |
| `NameTooLong`           | ник длиннее `MAX_NAME_LEN`              | `ник длиннее 20 символов`             |
| `NameTaken(String)`     | ник уже занят (внутри нормализованный)  | `ник alice уже занят`                 |
| `SelfFollow`            | подписка на себя                        | `нельзя подписаться на самого себя`   |
| `UnknownUser(UserId)`   | пользователя с таким id нет             | `нет пользователя с id 7`             |
| `UnknownPost(PostId)`   | поста с таким id нет                    | `нет поста с id 7`                    |
| `EmptyPost`             | пост пустой после обрезки пробелов      | `пост не может быть пустым`           |
| `PostTooLong`           | пост длиннее `MAX_POST_LEN`             | `пост длиннее 280 символов`           |

Числа 20 и 280 в сообщениях берутся из констант (`write!(f, "ник длиннее {MAX_NAME_LEN} символов")`), а не пишутся руками дважды.

## Дерево после вехи

```
backend/
  Cargo.toml
  src/
    lib.rs
    domain/
      mod.rs
      error.rs            DomainError, Display, Error
      user.rs             + MAX_NAME_LEN, validate_name
      post.rs             + MAX_POST_LEN, validate_text
    network/
      mod.rs              операции с Result, поиск с Option
      follows.rs
      tags.rs
  tests/                  тесты курса, не правь
    m01_user_post.rs
    m02_follow_graph.rs   тесты вехи 2 под новый Result
    m03_domain_errors.rs
```

## Приёмка

- `tests/m02_follow_graph.rs`: прежнее поведение, но операции отдают `Ok(...)`.
- `tests/m03_domain_errors.rs`: каждое правило из таблицы, сеть не меняется после ошибки, поиск отдаёт `Some` и `None`, `Display` печатает ровно фразы из таблицы, `DomainError` годится в `Box<dyn Error>`, цепочка операций проходит через `?`.
- Архитектура: всё из прошлой вехи, плюс есть `src/domain/error.rs`, а в `src/` нет `.unwrap()` и `panic!`.

## Как запустить

```
cd backend
cargo test
```
