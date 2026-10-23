# Экзамен: служба доставки вебхуков

Магазины, CRM и соцсети умеют сообщать другим системам о событиях: «оплачен заказ», «новый подписчик». Делают это вебхуками: на адрес, который указал получатель, уходит POST с описанием события. Звучит как одна строчка кода, но в бою всё ломается по-разному: получатель лежит, отвечает через минуту, отвечает 429, принимает событие дважды, путает порядок, а подделать запрос от тебя может любой, кто знает адрес. Твоя задача: написать библиотеку `hookd`, которая доставляет события честно. Подсказок нет, это экзамен по всему разделу: слои, конфигурация и секреты, проверка входа, ошибки API, ключи доступа, идемпотентность, фоновая доставка с повторами, устойчивость к зависшим получателям, метрики и мягкая остановка.

Сеть в тестах поддельная: всё, что уходит наружу, проходит через типаж `Transport`, а время заморожено (`start_paused`). Docker, базы и NATS не нужны.

## Что надо создать

Папка пуста, кроме приёмочного теста. Создай сам:

```
svdep_exam/
  Cargo.toml              один пакет `hookd` со своим пустым [workspace]
  src/lib.rs              только объявления модулей и реэкспорт
  src/config.rs           Config, ConfigError
  src/dto.rs              SubscribeRequest, FieldError, проверки
  src/error.rs            ApiError
  src/sign.rs             подпись
  src/service.rs          Hookd, Transport и всё, что связано с доставкой
  tests/acceptance.rs     приёмочные тесты (уже лежат, менять нельзя)
```

Редакция 2024. Зависимости ровно такие:

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
rand = "0.8"

[dev-dependencies]
tokio = { version = "1", features = ["full", "test-util"] }
```

## Публичный API

Всё ниже доступно из корня крейта (`use hookd::...`).

```rust
pub struct Config {
    pub signing_key: String,
    pub max_attempts: u32,
    pub backoff: Duration,
    pub max_backoff: Duration,
    pub attempt_timeout: Duration,
}
impl Config {
    pub fn from_env(vars: &HashMap<String, String>, read_file: impl Fn(&str) -> io::Result<String>)
        -> Result<Config, ConfigError>;
}
pub enum ConfigError { Missing(String), Ambiguous(String), Unreadable(String), Empty(String), Invalid { name: String, value: String } }

pub struct SubscribeRequest { pub url: String, pub events: Vec<String> }   // Deserialize
pub struct FieldError { pub path: String, pub code: &'static str }

pub enum ApiError { Unauthorized, Validation(Vec<FieldError>), IdempotencyConflict, ShuttingDown }
impl ApiError {
    pub fn status(&self) -> u16;
    pub fn code(&self) -> &'static str;
    pub fn body(&self) -> serde_json::Value;
}

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
pub struct Outgoing { pub url: String, pub headers: Vec<(String, String)>, pub body: Vec<u8> }
impl Outgoing { pub fn header(&self, name: &str) -> Option<&str>; }
pub struct TransportError(pub String);
pub trait Transport: Send + Sync + 'static {
    fn send(&self, request: Outgoing) -> BoxFuture<Result<u16, TransportError>>;
}

pub struct Published { pub id: String, pub duplicate: bool }
pub struct DeadLetter { pub event_id: String, pub subscription: String, pub attempts: u32, pub last_status: Option<u16> }
pub struct Stats { pub published: u64, pub delivered: u64, pub failed_attempts: u64, pub dead: u64 }
pub struct ShutdownReport { pub drained: bool, pub undelivered: usize }

pub struct Hookd;
impl Hookd {
    pub fn new(config: Config, transport: Arc<dyn Transport>) -> Hookd;
    pub fn create_tenant(&self, name: &str) -> String;
    pub fn subscribe(&self, key: &str, request: SubscribeRequest) -> Result<String, ApiError>;
    pub fn publish(&self, key: &str, idempotency_key: &str, kind: &str, data: serde_json::Value)
        -> Result<Published, ApiError>;
    pub fn dead_letters(&self, key: &str) -> Result<Vec<DeadLetter>, ApiError>;
    pub fn stats(&self) -> Stats;
    pub async fn shutdown(&self, grace: Duration) -> ShutdownReport;
}
```

Типам данных нужны `Debug`, `Clone` и `PartialEq` там, где тесты их сравнивают (`Config`, `ConfigError`, `FieldError`, `ApiError`, `Outgoing`, `Published`, `DeadLetter`, `Stats`, `ShutdownReport`), у `Stats` ещё `Copy` и `Default`.

## Конфигурация

`Config::from_env` получает переменные окружения словарём и функцию чтения файлов (в тестах подмена).

- Ключ подписи: ровно одна из переменных `HOOKD_SIGNING_KEY` (значение) и `HOOKD_SIGNING_KEY_FILE` (путь к файлу). Обе сразу: `Ambiguous`, ни одной: `Missing`, файл не читается: `Unreadable`. Из содержимого файла снимается один перевод строки на конце (`\n` или `\r\n`). Пустой ключ: `Empty`. Во всех этих ошибках имя `"HOOKD_SIGNING_KEY"`.
- Числа, все необязательные, целые в указанных пределах включительно:

  | переменная | по умолчанию | пределы |
  |---|---|---|
  | `HOOKD_MAX_ATTEMPTS` | 5 | 1..20 |
  | `HOOKD_BACKOFF_MS` | 200 | 1..60000 |
  | `HOOKD_MAX_BACKOFF_MS` | 10000 | 1..600000 |
  | `HOOKD_ATTEMPT_TIMEOUT_MS` | 2000 | 1..60000 |

  Нечисло или выход за пределы: `Invalid { name, value }` с исходной строкой значения. Потолок паузы меньше самой паузы: `Invalid` с именем `HOOKD_MAX_BACKOFF_MS`.
- `Debug` у `Config` не показывает ключ подписи, остальные поля показывает.

## Арендаторы и ключи

`create_tenant(name)` выдаёт новый ключ доступа вида `hk_` и 32 шестнадцатеричных символа из криптостойкого генератора. Хранить сам ключ нельзя, только его хеш SHA-256. Каждый метод с параметром `key` сначала узнаёт арендатора по ключу; неизвестный ключ это `Unauthorized`. Арендаторы не видят чужого: публикация уходит только подпискам своего арендатора, `dead_letters` отдаёт только свои записи.

## Проверка входа

`subscribe` проверяет запрос и возвращает **все** ошибки сразу, в порядке полей:

- `url`: начинается с `https://` (иначе код `scheme`); после схемы есть непустое имя хоста до `/`, `?` или `#` (иначе `host`); длина не больше 2048 байт (иначе `too_long`);
- `events`: пустой список это `events`/`empty`, больше 20 элементов это `events`/`too_many`;
- каждый элемент `events[i]`: либо `*` (все события), либо тип события вида `group.action`, где обе части из строчных латинских букв и `_` и начинаются с буквы (иначе `format`); повтор уже встреченного значения это `duplicate`.

`publish` проверяет свои аргументы и тоже собирает все ошибки: `idempotency_key` это 1..128 видимых ASCII-символов без пробелов (иначе `idempotency_key`/`format`), `kind` это тип события вида `group.action` (иначе `type`/`format`, звёздочка здесь недопустима).

## Ошибки API

| вариант | status | code |
|---|---|---|
| `Unauthorized` | 401 | `unauthorized` |
| `Validation(..)` | 400 | `validation` |
| `IdempotencyConflict` | 409 | `idempotency_conflict` |
| `ShuttingDown` | 503 | `shutting_down` |

`body()` возвращает `{"error": {"code": ..., "message": ...}}`, где `message` это `Display` ошибки (текст на твой вкус), а у `Validation` ещё `"fields": [{"path": ..., "code": ...}, ...]`. У остальных ошибок поля `fields` нет совсем.

## Публикация и идемпотентность

Порядок проверок в `publish`: ключ доступа, остановка (`ShuttingDown`), проверка аргументов, идемпотентность.

- Новое событие получает идентификатор `evt_<n>` (сквозная нумерация с 1), ответ `Published { id, duplicate: false }`, счётчик `published` растёт.
- Повтор с тем же `idempotency_key` у того же арендатора, тем же `kind` и теми же данными возвращает тот же `id` с `duplicate: true` и ничего не доставляет повторно. Данные сравниваются по смыслу: порядок ключей в объекте JSON не важен.
- Тот же ключ с другим `kind` или другими данными: `IdempotencyConflict`.
- Ключи идемпотентности у каждого арендатора свои.
- Событие встаёт в очередь каждой подписки арендатора, у которой в `events` есть `*` или ровно этот тип. Подписок может не быть, это не ошибка.

`subscribe` возвращает идентификатор `sub_<n>` (сквозная нумерация с 1). Порядок проверок: ключ, остановка, проверка запроса.

## Доставка

Доставка идёт в фоне, `publish` её не ждёт. У каждой подписки своя очередь и свой порядок:

- События одной подписки уходят строго по порядку публикации. Следующее начинает доставляться только тогда, когда судьба предыдущего решена (доставлено или ушло в dead letter).
- Подписки независимы: зависший или медленный получатель не задерживает другие подписки ни на миг.

Одна попытка это вызов `Transport::send` с запросом:

- `url` подписки;
- тело: JSON `{"id": <id события>, "type": <тип>, "data": <данные>}`, одинаковые байты во всех попытках;
- заголовки `content-type: application/json`, `x-hookd-id`, `x-hookd-event` (тип), `x-hookd-attempt` (номер попытки с 1), `x-hookd-signature: sha256=<hex>`, где hex это HMAC-SHA256 ключом подписи от точных байт тела, строчными буквами. HMAC пишешь сам поверх `sha2` (RFC 2104).

Итог попытки:

- статус 2xx: доставлено, счётчик `delivered`;
- статус 429, 5xx, ошибка транспорта или попытка дольше `attempt_timeout`: неудача, которую стоит повторить;
- любой другой статус (3xx, 4xx кроме 429): окончательная неудача, повторов нет.

Каждая неудачная попытка увеличивает `failed_attempts`. Перед повтором номер `n` (первый повтор это `n = 1`) пауза `backoff · 2^(n-1)`, но не больше `max_backoff`. Всего попыток не больше `max_attempts`. Окончательная неудача или исчерпанные попытки кладут запись в dead letter: идентификаторы события и подписки, число сделанных попыток и статус последней попытки (`None`, если её итогом была ошибка транспорта или срок). Счётчик `dead` растёт.

## Остановка

`shutdown(grace)`:

1. сразу перестаёт принимать новое: `publish` и `subscribe` отвечают `ShuttingDown` (после проверки ключа);
2. ждёт, пока все очереди опустеют и доставки закончатся, но не дольше `grace`;
3. останавливает фоновую доставку: после возврата ни одной новой попытки не будет;
4. возвращает `drained: true`, если всё успели, и число событий, оставшихся в очередях и в работе.

Остановка не ждёт зависшую попытку дольше `grace`: срок общий.

## Как проверить

```
cargo test
cargo clippy --all-targets -- -D warnings
```

Тесты ловят и «ленивые» решения: паузы проверяются по замороженным часам с точностью до миллисекунды, порядок и независимость подписок проверяются зависшими получателями, подпись сверяется с HMAC, посчитанной в тесте отдельно.
