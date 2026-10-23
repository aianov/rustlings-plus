# DTO соцсети на своём валидаторе

Соцсеть принимает от клиентов три вещи: регистрацию, правку профиля (PATCH) и новый пост с тегами и
ссылками. Для каждой нужен путь от сырого JSON до проверенных значений: serde, нормализация, правила,
доменный тип или список ошибок с путями. Крейт `validation` из прошлого проекта уже лежит готовый, осталось
написать на нём крейт `social-dto`.

## Что уже есть

- Воркспейс из двух крейтов: готовый `src/crates/validation` (править не нужно) и пустой `src/crates/social-dto`
  с нужными зависимостями (`validation`, `serde`, `serde_json`).
- Приёмочные тесты `src/crates/social-dto/tests/acceptance.rs`. Править их нельзя.

## Что сделать

В `src/crates/social-dto/src/lib.rs` напиши (всё `pub`):

1. `parse_register(json: &str) -> Result<Registration, Vec<FieldError>>`.
   `Registration { username: Username, email: Email, display_name: Option<String>, birth_year: Option<u16>, marketing_opt_in: bool }`
   (`Debug`, `PartialEq`). Тело в camelCase: `username`, `email`, `displayName`, `birthYear`, `marketingOptIn`.
   `displayName` нормализуется (невидимые символы U+200B, U+200C, U+200D, U+2060, U+FEFF убираются, повторные пробелы
   схлопываются) и проверяется на 1..=30 символов; `birthYear` в 1900..=2020; `marketingOptIn` по умолчанию `false`.
   Порядок ошибок: username, email, displayName, birthYear.
2. `apply_profile_patch(profile: &mut Profile, json: &str) -> Result<(), Vec<FieldError>>`.
   `Profile { display_name: String, bio: Option<String>, birth_year: Option<u16> }` (`Debug`, `Clone`, `PartialEq`).
   PATCH с тремя состояниями поля: не прислано (не трогать), `null` (очистить `bio` и `birthYear`; для `displayName`
   ошибка `null_not_allowed`), значение (проверить: `displayName` 1..=30 после нормализации, `bio` 1..=160, `birthYear`
   1900..=2020; строки хранятся обрезанными). При любой ошибке профиль не меняется.
3. `parse_post(json: &str) -> Result<NewPost, Vec<FieldError>>`.
   `NewPost { text: String, tags: Vec<String>, links: Vec<Link> }`, `Link { url: String, title: Option<String> }`
   (`Debug`, `PartialEq`). `text` 1..=280 символов после trim (хранится обрезанным); `tags` необязательны, не больше 10,
   каждый нормализуется (trim, без ведущих `#`, пробелы внутри заменяются на `-`, нижний регистр) и должен быть
   1..=30 символов, повтор после нормализации это ошибка `duplicate` у второго вхождения; `links` необязательны, не больше
   5, у каждой `url` начинается с `http://` или `https://`, 1..=200 символов, `title` до 50.

Общие правила:

- Лишние поля в теле это ошибка `unknown_field` с путём, равным имени поля (несколько лишних идут по алфавиту).
- Битый JSON, не объект, пропущенное обязательное поле или неверный тип это одна ошибка с пустым путём и кодом `bad_json`.
- Ошибки проверки собираются все, пути вида `tags[1]`, `links[0].url`; порядок полей: как в теле выше.
- Типизированные значения (`Username`, `Email`) создаются только через `validation`.

## Критерии приёмки

`cargo test --workspace` проходит и `cargo clippy --workspace --all-targets -- -D warnings` чист.

## Подсказки

README главы: «Конвейер проверки», «PATCH». Крейт `validation` умеет почти всё, что нужно: правила, пути, конструкторы.
