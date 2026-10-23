# Serde и данные на проводе

Метеостанция на крыше присылает показания: температуру, влажность, время. Приложение на телефоне показывает их
в списке, и однажды список опустел: станцию обновили, поле `temp_c` стало называться `tempC`, и приложение молча
пропустило все записи. Никто не ошибся в логике, ошибся контракт: формат на проводе это тоже API, и менять его
надо так же осторожно, как публичную функцию. Serde делает так, чтобы контракт был описан один раз, рядом с типом,
а проверял его компилятор и тесты. Две анимации (клавиша `v`) показывают путь значения через serde и четыре
способа записать enum.

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Reading {
    station: String,
    celsius: f32,
    at: u64,
}

let json = serde_json::to_string(&Reading { station: "roof".into(), celsius: 21.5, at: 1700000000 }).unwrap();
// {"station":"roof","celsius":21.5,"at":1700000000}
let back: Reading = serde_json::from_str(&json).unwrap();
```

В `Cargo.toml` для этого нужны `serde = { version = "1", features = ["derive"] }` и `serde_json = "1"`.

## Как устроен serde

Serde ничего не знает о JSON. У него есть **модель данных**: числа, строки, последовательности, словари, `Option`,
структуры, варианты enum, около тридцати видов. `derive(Serialize)` пишет код, который обходит поля типа и
сообщает формату: «тут `u64`, тут строка». `derive(Deserialize)` пишет «посетителя» (`Visitor`), которому формат
отдаёт найденное. А формат (`serde_json`, `toml`, `bincode`) превращает вызовы в текст или байты.

```
 твой тип ──Serialize──▶ модель данных ──serde_json──▶ {"station":"roof",..}
 твой тип ◀─Deserialize─ модель данных ◀─serde_json─── {"station":"roof",..}
              ▲
      атрибуты (#[serde(..)]) вмешиваются здесь: имена, пропуски, свои функции
```

Из этого следуют два правила. Атрибуты описывают тип, а не JSON, поэтому те же атрибуты работают для любого формата.
А ошибки чтения делятся на «не JSON вообще» и «JSON, но не твоего вида», и это разные ошибки (ниже про `classify`).

Основные функции `serde_json`: `to_string`, `to_vec`, `to_string_pretty` (записать), `from_str`, `from_slice`,
`from_reader` (прочитать). Записать почти всегда можно без ошибок, поэтому `to_string(..).expect("причина")`
с объяснением честнее ненужного `Result`. Читать чужие данные нужно всегда с обработкой ошибки.

## Имена и необязательные поля

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Alert {
    station_id: String,           // "stationId"
    #[serde(rename = "type")]     // "type": это слово в Rust зарезервировано
    kind: String,
    #[serde(alias = "lvl")]       // при чтении принимается и "lvl", пишется "level"
    level: u8,
}
```

`rename_all` задаёт общее правило для **полей** этого типа (значения: `camelCase`, `snake_case`, `PascalCase`,
`SCREAMING_SNAKE_CASE`, `kebab-case`, `lowercase`). Для имён вариантов enum и вложенных типов нужен свой `rename_all`:
правило не наследуется. `rename` переименовывает одно поле, `alias` добавляет дополнительное имя только для чтения.

Отсутствующее поле serde по умолчанию считает ошибкой. Исключение: `Option`, для него отсутствие ключа и `null`
дают `None`. Всё остальное решается атрибутами:

```rust
#[derive(Serialize, Deserialize)]
struct Config {
    #[serde(default = "default_interval")]       // функция fn() -> u32
    interval_s: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    labels: Vec<String>,
    #[serde(skip)]                                // ни в JSON, ни из JSON; тип обязан реализовать Default
    cache: Option<String>,
}
```

Важная ловушка: `skip_serializing_if` действует только при записи. Если клиент пропускает пустой список, читающая
сторона должна иметь `default`, иначе получит `missing field`. Пара «не пишем пустое» и «читаем отсутствие» всегда идёт
вместе. По умолчанию serde молча игнорирует незнакомые ключи, что хорошо для совместимости со старыми и новыми
версиями. Если нужна строгость, на типе ставят `#[serde(deny_unknown_fields)]`.

## Enum на проводе: четыре представления

```rust
#[derive(Serialize, Deserialize)]
enum Event { Rain { mm: f32 }, Wind(f32), Calm }
```

- **Внешний тег** (по умолчанию): `{"Rain":{"mm":2.5}}`, `{"Wind":4.0}`, `"Calm"`.
- **Внутренний тег** `#[serde(tag = "kind")]`: `{"kind":"Rain","mm":2.5}`, `{"kind":"Calm"}`. Как в большинстве API.
  Вариант-кортеж с числом или строкой так записать нельзя: рядом с голым числом нет объекта, куда положить тег, и запись
  возвращает ошибку (derive при этом компилируется!).
- **Соседний тег** `#[serde(tag = "t", content = "d")]`: `{"t":"Wind","d":4.0}`. Работает для любого варианта.
- **Без тега** `#[serde(untagged)]`: `{"mm":2.5}`, `4.0`. Вид определяется по форме данных.

`untagged` пробует варианты по порядку и берёт первый, у которого получилось. Лишние ключи по умолчанию
игнорируются, поэтому объект `{"mm":2.5,"city":"Riga"}` подойдёт и варианту `A { mm }`, и варианту
`B { mm, city }`, если `A` стоит первым. Порядок вариантов решает, поэтому его выбирают осознанно, а если форма данных неоднозначна, лучше явный тег.

Имена вариантов настраиваются через `rename_all` на самом enum, а отдельный вариант через `rename`.

## flatten

Общие поля удобно описывать один раз и вставлять в разные типы без вложенного объекта:

```rust
#[derive(Serialize, Deserialize)]
struct Header { seq: u64, at: u64 }

#[derive(Serialize, Deserialize)]
struct Packet {
    #[serde(flatten)] header: Header,
    #[serde(flatten)] payload: Event,      // enum с внутренним тегом
    #[serde(flatten)] rest: std::collections::BTreeMap<String, serde_json::Value>,   // всё остальное
}
// {"seq":1,"at":9,"kind":"Rain","mm":2.5,"firmware":"v2"}
```

Карта с `flatten` собирает все ключи, которые не взяли другие поля: так сервис, который лишь передаёт сообщение
дальше, не теряет незнакомые поля. Цена: при чтении объект с `flatten` сначала целиком буферизуется. Сочетание
`flatten` с `deny_unknown_fields` документация serde называет неподдерживаемым: проверка может срабатывать иначе, чем
ожидаешь (например, сообщение не перечислит допустимые поля). Не полагайся на это для защиты, проверяй явно.

`flatten` работает и вокруг обобщённого типа: `struct Reply<T> { id: u64, #[serde(flatten)] body: Outcome<T> }`, где
`Outcome<T>` это enum с вариантами `Ok(T)` и `Error(..)`. Границы `Serialize`/`Deserialize` на `T` выводятся сами.

## Newtype и свои форматы значений

Структура-кортеж с одним полем (`struct StationId(u64)`) на проводе выглядит как само значение: `42`. Для структуры
с одним именованным полем то же даёт `#[serde(transparent)]`. Когда вид на проводе сильно отличается от вида в
памяти (число в Rust, префикс на проводе), проще описать превращение обычными `From` и `TryFrom`:

```rust
#[derive(Clone, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
struct StationId(u64);          // на проводе "st_42"

impl From<StationId> for String { fn from(id: StationId) -> String { format!("st_{}", id.0) } }
impl TryFrom<String> for StationId {
    type Error = String;                                 // любой тип с Display
    fn try_from(s: String) -> Result<StationId, String> { /* разобрать префикс и число */ }
}
```

`into` требует `Clone`, потому что serde преобразует копию. Ошибка из `TryFrom` попадёт в сообщение разбора как есть.

Когда нужно изменить формат одного поля, а не всего типа, подключают функции атрибутами `serialize_with = "путь"`
и `deserialize_with = "путь"`, или одним `with = "модуль"`, где лежат функции `serialize` и `deserialize`:

```rust
mod seconds {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(d.as_secs())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        Ok(Duration::from_secs(u64::deserialize(d)?))
    }
}
// #[serde(with = "seconds")] interval: Duration
```

Свою ошибку при чтении создают так: `use serde::de::Error;` и `D::Error::custom("текст")`. Serde сам добавит позицию.
Для строк пригодятся `serializer.serialize_str(&text)` и `serializer.collect_str(&value)` (записывает любое значение с `Display` как строку).

## Большие числа и JavaScript

JSON не ограничивает размер числа, но JavaScript читает все числа как дробные двойной точности. Целые точны только
до 2^53 (9007199254740992), выше часть значений читается округлённо, и идентификатор превращается в чужой. Поэтому
всё, что может быть больше 2^53 (идентификаторы, деньги в минимальных единицах, время в наносекундах), отправляют
строкой. Читать лучше обе формы: так старые данные, где число ещё числом, не сломаются. Приём: в `deserialize`
заводят маленький enum на оба вида и вытаскивают число из любого:

```rust
#[derive(Deserialize)]
#[serde(untagged)]
enum NumOrText { Num(u64), Text(String) }

fn qty<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    match NumOrText::deserialize(d)? {
        NumOrText::Num(n) => Ok(n),
        NumOrText::Text(s) => s.parse().map_err(serde::de::Error::custom),
    }
}
```


## Value и двухэтапный разбор

Когда форма JSON заранее неизвестна, или тип зависит от соседнего поля, берут `serde_json::Value`: JSON как дерево
(`Null`, `Bool`, `Number`, `String`, `Array`, `Object`).

```rust
use serde_json::{json, Value};

let doc: Value = serde_json::from_str(raw)?;
let city = doc["station"]["city"].as_str();           // нет пути: Null, as_str вернёт None
let mut reply = json!({ "ok": true, "readings": [], "n": 0 });
reply["n"] = json!(3);                                // запись создаёт недостающие уровни
```

Читать по индексу можно всегда, путь не обязан существовать. Писать (`doc["a"]["b"] = ..`) можно только в объект или в `Null`: если на пути лежит число или строка, индексирование паникует. Значения достают методами `as_str`, `as_u64`, `as_array`: все возвращают `Option`.
В типизированное значение дерево превращает `serde_json::from_value(value)`, обратно `to_value`.

Типичный приём для протоколов: конверт, у которого тип полезной нагрузки зависит от поля-метода.

```rust
#[derive(Deserialize)]
struct Envelope { id: u64, method: String, data: Value }   // этап 1: данные пока дерево

match env.method.as_str() {                                // этап 2: тип выбирает метод
    "sensor.read" => { let q = Query::deserialize(&env.data)?; ... }
    other => return Err(UnknownMethod(other.to_string())),
}
```

`&Value` сам реализует `Deserializer`, поэтому `Query::deserialize(&env.data)` читает дерево по ссылке, без `clone()`.
Неизвестный метод и данные не той формы это ожидаемые ошибки, и их место в `Result`, а не в `panic!` и `unwrap`.

## Обобщённые конверты и DeserializeOwned

`struct Envelope<T> { id: u64, data: T }` с derive работает для любого `T`; derive сам добавляет границы
`T: Serialize` и `T: Deserialize`. В функциях границы пишешь ты, и тут есть тонкость:

```rust
fn parse<'a, T: Deserialize<'a>>(raw: &'a str) -> T { serde_json::from_str(raw).unwrap() }   // T может ссылаться на raw
fn load<T: DeserializeOwned>(path: &str) -> T { let raw = read(path); serde_json::from_str(&raw).unwrap() }   // T ничего не держит
```

`Deserialize<'de>` значит «умею читаться из данных, которые живут `'de`, и могу заимствовать у них». `DeserializeOwned`
это `for<'de> Deserialize<'de>`: «читаюсь из любого источника и ничего не заимствую». Если функция сама создаёт
строку (прочитала файл, получила тело ответа) и отдаёт `T` наружу, ей нужен `DeserializeOwned`: иначе `T`
мог бы ссылаться на строку, которая умирает при выходе.

## Заимствующий разбор

Если исходная строка живёт долго, поля можно не копировать:

```rust
#[derive(Deserialize)]
struct Line<'a> {
    station: &'a str,                 // срез исходного JSON, без аллокации
    #[serde(borrow)]
    note: std::borrow::Cow<'a, str>,  // срез, если в тексте нет экранирования, иначе копия
}
```

`&'a str` заимствует автоматически, но падает на строке с `\n` или `\"`: в исходнике там записано два знака, а нужен
один, срез невозможен («expected a borrowed string»). `Cow<'a, str>` решает это, но по умолчанию serde читает его в
`Cow::Owned`, поэтому на поле нужен `#[serde(borrow)]`: без него код работает и тихо копирует всё. Структура
с заимствованием привязана к исходной строке: пока живёт она, должна жить и строка. То же правило у `Option<&str>`
и `&[u8]` (заимствуют сами), а `Cow<[u8]>` без `#[serde(borrow)]` копирует, как и `Cow<str>`.

## Ошибки serde_json

У `serde_json::Error` есть `line()`, `column()` и `classify()`, которая возвращает `serde_json::error::Category`:

- `Eof`: текст оборвался (передача не закончилась, поэтому имеет смысл ждать продолжения);
- `Syntax`: это не JSON (лишняя запятая, незакрытая скобка, знак на месте значения);
- `Data`: это JSON, но не того вида (не хватает поля, строка вместо числа);
- `Io`: ошибка чтения из источника (при `from_str` не бывает).

Для клиента API эти три вида означают разные действия, поэтому их стоит различать, а текст причины и позицию хранить
отдельно. Строка `Display` у ошибки заканчивается на `at line X column Y`; чтобы получить чистую причину, этот хвост
отрезают. Ошибки из `from_value` и `T::deserialize(&value)` хвоста не имеют (разбор идёт по дереву, а не по тексту), и `line()` у них 0.

## Типичные ошибки

- `skip_serializing_if` без `default`: пишем без поля, читаем с ошибкой.
- `untagged` с похожими объектами в «неправильном» порядке: сообщение становится соседним вариантом без текста.
- Тег внутри enum, в котором есть вариант-кортеж с числом: ошибка записи.
- Большие идентификаторы числом для JavaScript-клиента.
- `Cow` без `#[serde(borrow)]`: работает, но всё копируется.
- `Deserialize<'a>` там, где строка создаётся внутри функции.
- `unwrap` и `panic!` при разборе чужих данных.
- `from_value(data.clone())` на каждый запрос: достаточно `T::deserialize(&data)`.
- Переименование полей без теста: контракт сломается молча.

## Что почитать

- serde.rs, разделы «Attributes» (container, variant, field), «Enum representations» и «Custom serialization».
- Документация `serde_json`: `Value`, `json!`, `Error::classify`.
- «Understanding deserializer lifetimes» на serde.rs: про `'de` и `DeserializeOwned`.
