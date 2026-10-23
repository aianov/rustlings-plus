# Крейт validation

В соцсеть добавляются новые формы, и в каждой те же проверки: длина, формат, диапазон, вложенные
части с путями ошибок. Писать их заново в каждой форме нельзя. Собери крейт `validation`: набор правил,
сборщик ошибок с путями, мини-шаблон и типы, которые создаются только валидными. Всё это ты уже писал по
кускам в задачах главы (особенно `svdto5`, `svdto7`, `svdto9`, `svdto10`, `svdto13`).

## Что уже есть

- Воркспейс с крейтом `src/crates/validation` (без зависимостей) и пустым `src/lib.rs`.
- Приёмочные тесты в `src/crates/validation/tests/acceptance.rs`. Править их нельзя.

## Что сделать

Модули свои, наружу всё экспортируется из корня крейта (`validation::Имя`).

1. `Pattern` и `PatternError { Unclosed, EmptyClass, BadRange, BadEscape, NothingToRepeat, BadQuantifier }`:
   мини-шаблон из задачи `svdto9` (литералы, `.`, классы с диапазонами и отрицанием, экранирование,
   `?` `*` `+` `{m}` `{m,n}`, совпадение целой строки, `is_match`, запоминание неудачных состояний).
2. `FieldError { path: String, code: &'static str, message: String }` (`Debug`, `Clone`, `PartialEq`, поля `pub`).
3. Трейт `Rule<T: ?Sized> { fn check(&self, value: &T) -> Option<(&'static str, String)> }`: `None` значит «правило
   выполнено», иначе пара «машинный код, сообщение».
4. Правила (все поля `pub`, где они есть):
   - `NotBlank` для `str`: пусто после trim, код `blank`;
   - `CharLen { min, max }` для `str`: длина в символах после trim, коды `too_short`, `too_long`;
   - `Matches` для `str`: `Matches::new(&str) -> Result<Matches, PatternError>`, код `invalid_format`;
   - `Range<N> { min, max }` для любого `N: PartialOrd + Display + Copy`, код `out_of_range`;
   - `MaxItems(usize)` для `[T]`, код `too_many`;
   - `OneOf(&'static [&'static str])` для `str`, код `not_allowed`.
5. `Validator`:
   - `new()`, `finish(self) -> Result<(), Vec<FieldError>>`;
   - `check(&mut self, field: &str, value: &T, rules: &[&dyn Rule<T>])`: запускает правила по порядку и
     записывает ошибку только от первого сработавшего;
   - `scope(&mut self, field: &str, f: impl FnOnce(&mut Validator))`: вложенная структура, путь `author.handle`;
   - `each(&mut self, field: &str, items: &[T], f: impl FnMut(&mut Validator, &T))`: элементы списка,
     путь `links[2].url`, вложения можно комбинировать;
   - `error(&mut self, field: &str, code: &'static str, message: impl Into<String>)`: своя ошибка по текущему пути.
6. `Username::parse(&str)` и `Email::parse(&str)` дают `Result<_, FieldError>`: значение нормализуется (trim,
   нижний регистр) и проверяется; у `FieldError` из них пустой `path`; есть `as_str()` и `Display`.
   Ник: 3..=20 символов, `[a-z][a-z0-9_]*`. Почта: один `@`, непустая локальная часть до 64 символов,
   домен из непустых меток через точку (меток не меньше двух), без пробелов.

## Критерии приёмки

`cargo test --workspace` проходит и `cargo clippy --workspace --all-targets -- -D warnings` чист. Тесты проверяют
шаблон (в том числе время на восьми `a*` подряд), правила, пути ошибок для вложенных структур и списков,
правило «одна ошибка на поле» и конструкторы типов.

## Подсказки

README главы: «Сбор ошибок», «Пути», «Мини-шаблон», «Parse, don't validate». Начинай с `Pattern`: на нём
стоят `Matches` и конструкторы.
