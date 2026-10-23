# Ошибки как часть дизайна крейта

Пользователь загружает фото в приложение, а получает на экране: `No such file or directory (os error 2)`.
Какой файл? Это превью, оригинал или кэш? Разработчик открывает лог и видит ту же строку. Ошибка сама по себе
правильная, но на трёх уровнях кода (чтение файла, декодер картинки, загрузчик) никто не добавил ни слова о том,
что в этот момент делал. Хорошая ошибка рассказывает цепочку: «не удалось загрузить фото: не удалось прочитать
`/tmp/avatar.png`: нет такого файла». Анимация (клавиша `v`) показывает, как такая цепочка собирается и читается.

В `rust-basics` ты уже видел `Result`, `?` и свой enum. Здесь про проектирование: что и где хранить, как не терять
причину, что показывать наружу библиотеки и когда не возвращать ошибку вообще.

## Свой тип ошибки

Ошибка это обычное значение, и лучшая её форма это enum с данными в вариантах:

```rust
use std::fmt;

#[derive(Debug)]
pub enum UploadError {
    TooBig { bytes: u64, limit: u64 },
    UnknownFormat(String),
    Empty,
}

impl fmt::Display for UploadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UploadError::TooBig { bytes, limit } => write!(f, "file is {bytes} bytes, limit is {limit}"),
            UploadError::UnknownFormat(ext) => write!(f, "unknown format {ext:?}"),
            UploadError::Empty => write!(f, "file is empty"),
        }
    }
}

impl std::error::Error for UploadError {}
```

Три разные роли у трёх частей. Варианты и их данные для кода: `match` выбирает реакцию (ответить 413, а не 500).
`Display` для человека: одна строка про этот уровень. `Debug` для разработчика, обычно `derive`. Трейт
`std::error::Error` требует `Debug + Display` и открывает ящик: ошибка становится `Box<dyn Error>`, к ней
применимы `source` и `downcast_ref`.

## Option и Result: короткие переходы

Прежде чем ошибка появится, нужно превратить «значения может не быть» в «значения нет, потому что…».

```rust
let name = users.get(&id).ok_or_else(|| LookupError::NoUser(id))?;   // Option -> Result
let port: Option<u16> = settings.get("port").map(|s| s.parse()).transpose()?;   // Option<Result> -> Result<Option>
```

`ok_or(e)` строит ошибку всегда, даже когда значение есть. Если ошибка дорогая (формат строки, аллокация),
берут `ok_or_else(|| ...)`: замыкание вызывается только на пути ошибки. `map_err` меняет тип или содержимое
ошибки. `and_then` склеивает шаги, каждый из которых может не получиться. `transpose` меняет порядок слоёв:
«необязательное значение, которое может быть испорчено» это `Option<Result<T, E>>`, а удобнее работать с
`Result<Option<T>, E>`: нет значения это нормально, испорченное значение ошибка.

`?` работает и на `Option`, но только в функциях, которые возвращают `Option`. Из `Option` в `Result` он сам не
превращает: `None` не знает, какую ошибку подставить.

## source: ошибка помнит причину

```rust
#[derive(Debug)]
pub struct DecodeError {
    pub path: String,
    pub source: std::io::Error,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot decode {}", self.path)
    }
}

impl std::error::Error for DecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
```

Метод `source` по умолчанию возвращает `None`. Если его не переопределить, код соберётся, а причина молча
пропадёт из всех отчётов. Это самая тихая ошибка главы.

Правило для `Display`: он говорит только о своём уровне. Если уровень вставляет `{source}` в свой текст, а печать
потом ходит по `source`, причина появится дважды. Печать цепочки пишется один раз:

```rust
fn chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut parts = vec![err.to_string()];
    let mut current = err.source();
    while let Some(cause) = current {
        parts.push(cause.to_string());
        current = cause.source();
    }
    parts.join(": ")
}
```

Тот же цикл без накопления даёт корневую причину. `'static` в типе нужен для `downcast_ref`, о нём ниже.

```
 Display верхнего           "cannot upload"
        │ source()
        ▼
 Display среднего           "cannot decode a.png"
        │ source()
        ▼
 Display корня              "No such file or directory"
        │ source()
        ▼
      None
```

## From и ?: удобство и цена

`?` разворачивается в `match`, где ошибка проходит через `From::from`. Реализуешь `From<io::Error> for MyError`, и `?`
в функциях с `MyError` превращает ошибки ввода-вывода сама:

```rust
impl From<std::io::Error> for UploadFailure {
    fn from(e: std::io::Error) -> UploadFailure { UploadFailure::Io(e) }
}

fn read(path: &Path) -> Result<Vec<u8>, UploadFailure> {
    Ok(std::fs::read(path)?)
}
```

Цена в том, что `From` не знает, где его вызвали. Какой файл, какая строка, какой запрос: всего этого в `From`
нет. Ещё хуже, когда нужны два варианта с одним исходным типом (`Config(io::Error)` и `Data(io::Error)`):
`From` может быть только один. Если контекст нужен, ошибку строят на месте:

```rust
let bytes = std::fs::read(path).map_err(|source| UploadFailure::Read { path: path.display().to_string(), source })?;
```

Удобный компромисс: `From` оставляют там, где достаточно типа (разбор числа), и убирают там, где ошибке нужен
контекст, который знает только вызывающий.

## Контекст как трейт-расширение

Писать каждый раз `map_err(|source| Variant { path, source })` утомительно. Часто хватает универсальной обёртки:
сообщение и причина. Это `Result`, которому добавили метод `.context("...")`:

```rust
pub trait Context<T> {
    fn context(self, message: &str) -> Result<T, Contextual>;
    fn with_context<F: FnOnce() -> String>(self, message: F) -> Result<T, Contextual>;
}
```

Реализуют его для любого `Result<T, E>`, где `E: Error + Send + Sync + 'static`. Внутри `Contextual` хранит
`Box<dyn Error + Send + Sync>` и сообщение. Ленивый вариант `with_context(|| format!(...))` строит строку,
только если ошибка случилась. Так устроены популярные крейты вроде `anyhow`, а ты можешь написать то же в десять строк.

## Box<dyn Error + Send + Sync>: ошибка приложения

Библиотеке нужны типы ошибок: по ним решают, что делать. Бинарнику (самому верхнему слою) обычно достаточно
«что-то не получилось, вот история». Для этого тип `Box<dyn Error + Send + Sync>`:

```rust
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let n: u32 = "12".parse()?;                      // ParseIntError превратилась в коробку сама
    if n > 10 { return Err("too many".into()); }    // строка тоже
    Ok(())
}
```

Если `Send + Sync` не написать, такую ошибку нельзя вернуть из потока или передать между задачами async. В
публичных сигнатурах пиши оба слова сразу. Одной «неотправляемой» детали хватает, чтобы вся ошибка перестала быть
`Send`: поле с `Rc` делает свой тип не `Send`. Бери `Arc`.

Внутри коробки лежит настоящая ошибка. Достать её можно через `downcast_ref`:

```rust
if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
    if io_err.kind() == std::io::ErrorKind::NotFound { /* тихо подставить умолчание */ }
}
```

Но `downcast_ref` смотрит на тип самого значения, а не на цепочку. Если файл не найден, а ошибка завёрнута в
контекст, верхний тип будет `Contextual`. Чтобы найти причину на любой глубине, `downcast_ref` вызывают в цикле
по `source()`. Для этого типу нужно `'static`.

## Граница между крейтами

У каждого крейта свой `Error` и свой алиас:

```rust
pub type Result<T> = std::result::Result<T, Error>;
```

Так в сигнатуре видно, чья это неудача, и `?` внутри крейта пишется коротко. На границе нижний слой оборачивается
в вариант верхнего (`From<storage::Error> for Error`), и `?` пересекает границу сам.

Главное правило границы: публичная ошибка не называет типы чужих библиотек. Вариант `Parse(serde_json::Error)`
в публичном enum означает, что любое обновление `serde_json` может сломать всех, кто делает `match` по твоей
ошибке. Дай собственный вариант с нужными данными (строка, столбец, текст) и держи чужую ошибку внутри. Компилятор
отчасти поможет: линт `private_interfaces` ругается, когда публичное API называет тип с меньшей видимостью.

## Не терять ошибки

```rust
let _ = writeln!(log, "started");     // ошибка записи исчезла
writeln!(log, "started").ok();        // и здесь
let _ = log.flush();                  // и здесь: буфер не сброшен, а мы не узнаем
```

`let _ =` и `.ok()` заставляют компилятор замолчать, но не обрабатывают ошибку. Диск переполнен, журнал пуст, а
программа бодро рапортует об успехе. Если ничего сделать нельзя, это решение пишется явно (хотя бы
`if let Err(e) = ... { eprintln!(...) }`). Чаще ошибку возвращают через `?`.

Обратная сторона: значение, которое нельзя забывать. `Result` уже помечен `#[must_use]`, и компилятор
предупреждает, когда его бросают. Свой тип можно пометить так же:

```rust
#[must_use = "builder returns a new value"]
pub struct Query { /* ... */ }
```

Для сборщиков, которые возвращают изменённую копию, это спасает от `q.limit(10);`, после которого запрос остался прежним.

## panic или Result

`Result` для всего, что может пойти не так в нормальной работе: ввод пользователя, файлы, сеть, чужие данные.
Паника для нарушенных инвариантов, то есть ошибок программиста: индекс, который мы сами сделали корректным,
регулярное выражение из литерала. В таких местах пишут `expect("почему это невозможно")`: сообщение
объясняет, какое допущение нарушено.

Паника в потоке не обязательно убивает процесс: `thread::spawn(..).join()` возвращает `Err(payload)`. В `payload`
лежит `Box<dyn Any + Send>`, и сообщение достаётся двумя попытками: `downcast_ref::<&str>()` для паники с
литералом (`panic!("boom")`) и `downcast_ref::<String>()` для паники с форматированием (`panic!("bad {x}")`). Любая
другая нагрузка (через `panic_any`) текста не даёт.

## Типичные ошибки

- Забытый `source`: код собирается, а в отчётах причины нет.
- Display, который вставляет текст причины, плюс обход по `source`: всё печатается дважды.
- Один общий `From` для типа, которому нужен контекст (номер строки, имя файла).
- Чужой тип в публичной ошибке.
- `Box<dyn Error>` без `Send + Sync` в многопоточном коде.
- `downcast_ref` без обхода цепочки: находит только верхний уровень.
- `let _ =` и `.ok()` на записи и отправке.
- `unwrap` на данных, которые пришли снаружи.

## Что почитать

- Документация трейта `std::error::Error` (методы `source`, `downcast_ref` у `dyn Error`).
- The Rust Programming Language, глава 9 «Error Handling».
- Rust API Guidelines, раздел про осмысленные типы ошибок (C-GOOD-ERR).
- Исходники крейтов `thiserror` и `anyhow`: после этой главы читаются легко.
