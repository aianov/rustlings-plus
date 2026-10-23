# Async в трейтах и типах

Сервис выкладывает фотографии и сначала складывает их на локальный диск. Через месяц нужен
S3, ещё через месяц в тестах нужна подделка, которая отвечает с задержкой. Код сервиса при этом
меняться не должен: он зовёт `storage.put(..)` и не знает, что у него под ногами. Классический
ответ на это трейт, но у трейта с асинхронными методами есть две особенности, на которых
спотыкаются все. Первая: не любой такой трейт можно спрятать за `dyn`. Вторая: tokio любит `Send`,
а в трейте его легко потерять. Ошибки, которые отсюда выходят («future cannot be sent between
threads safely», «the trait is not dyn compatible», «borrowed data escapes outside of method»),
наполовину напоминают шифровку. Разберём, откуда они берутся.

## async fn в трейте

С версии Rust 1.75 метод трейта можно объявить через `async fn`, и никаких внешних крейтов не нужно:

```rust
trait Storage {
    async fn put(&self, name: &str, data: Vec<u8>) -> Result<(), StorageError>;
    async fn exists(&self, name: &str) -> bool;
}

struct LocalDisk { root: PathBuf }

impl Storage for LocalDisk {
    async fn put(&self, name: &str, data: Vec<u8>) -> Result<(), StorageError> {
        tokio::fs::write(self.root.join(name), data).await.map_err(StorageError::Io)
    }
    async fn exists(&self, name: &str) -> bool { /* ... */ }
}

async fn upload<S: Storage>(storage: &S, name: &str, data: Vec<u8>) -> bool {
    storage.put(name, data).await.is_ok()
}
```

Сигнатура в трейте это сокращение. На самом деле метод превращается в

```rust
fn put(&self, name: &str, data: Vec<u8>) -> impl Future<Output = Result<(), StorageError>>;
```

У каждой реализации здесь свой собственный анонимный тип future: у диска один, у S3 другой, у
подделки третий. В обобщённой функции вроде `upload<S: Storage>` это не мешает: `S` известен, а с ним
и всё остальное. А вот на двух вещах споткнёмся.

## Первая проблема: Send

Допустим, `upload` надо запустить в отдельной задаче:

```rust
fn upload_in_background<S>(storage: Arc<S>, name: String, data: Vec<u8>)
where
    S: Storage + Send + Sync + 'static,
{
    tokio::spawn(async move { storage.put(&name, data).await });
}
```

Компилятор откажет: *future cannot be sent between threads safely*. `tokio::spawn` может
выполнить задачу на любом потоке, значит, future обязан быть `Send`. Про future метода `put` трейт
знает только «какой-то future»: может, он держит `Rc` через `.await`, а может, нет. Для конкретных
типов компилятор проверит, для обобщённых молчание трейта значит «не обещано».

Лекарство в том, чтобы записать обещание в самом трейте. Метод объявляется обычным `fn`, который
возвращает `impl Future ... + Send`:

```rust
trait Storage: Sync {
    fn put(&self, name: &str, data: Vec<u8>) -> impl Future<Output = Result<(), StorageError>> + Send;
}
```

Реализации по-прежнему пишутся через `async fn`: компилятор сам проверит, что получающийся future
действительно `Send`, и сообщит об ошибке в месте реализации, а не где-то в `spawn`. Супертрейт
`Sync` нужен, если у трейта есть готовые методы (с телом): их future держит `&self`, а `&Self`
отправляется между потоками только когда `Self: Sync`.

Не всегда `spawn` вообще нужен. Если надо просто делать несколько вызовов одновременно, `join!` и
`join_all` из прошлой главы работают внутри одной задачи и ничего не требуют от `Send`.

### Что делает future не-Send

Future хранит всё, что живёт через `.await`: локальные переменные, аргументы, временные значения.
Если хоть одно из них не `Send`, не `Send` и сам future. Обычные виновники:

- `Rc`, `RefCell` с `Rc`, сырые указатели;
- охранник `std::sync::MutexGuard` (его нельзя отправлять между потоками);
- ссылка на что-то не `Sync`.

Смотри внимательно, до какой точки живёт значение. Компилятор в этом вопросе осторожен:
переменная считается живой до конца своей области видимости, даже если ты передал её в `drop(..)`.

```rust
async fn bad() {
    let guard = state.lock().unwrap();
    let n = guard.len();
    drop(guard);                  // не помогает: компилятор считает, что guard жив до конца блока
    network_call().await;
}

async fn good() {
    let n = { state.lock().unwrap().len() };   // охранник умер в конце блока
    network_call().await;
}
```

Чтобы такие ошибки находились рядом с причиной, в тестах ставят проверку на этапе компиляции:

```rust
fn assert_send<T: Send>(_: &T) {}

#[test]
fn handlers_are_send() {
    assert_send(&handle_upload(request()));
}
```

Если future перестанет быть `Send`, тест не соберётся и скажет, какой именно метод виноват.

## Вторая проблема: dyn

Хочется хранить разные хранилища вместе: `Vec<Arc<dyn Storage>>`. С `async fn` в трейте так нельзя,
компилятор скажет *the trait is not dyn compatible*. Для `dyn Storage` у каждого метода должен быть один
известный возвращаемый тип, а тут анонимный тип у каждой реализации свой. То же верно и для метода с
`-> impl Future`: это та же история, просто записанная явно. Вторая причина запрета на `dyn`: обобщённый
метод (`fn put<T>(..)`). Для вызова через таблицу виртуальных методов нужна одна готовая версия
метода, а обобщённый метод порождает по версии на каждый `T`. Упакованный future этого не лечит.

Выход: заменить анонимный тип одним общим, упакованным в `Box`:

```rust
use std::future::Future;
use std::pin::Pin;

trait Storage: Send + Sync {
    fn put<'a>(&'a self, name: &'a str, data: Vec<u8>)
        -> Pin<Box<dyn Future<Output = Result<(), StorageError>> + Send + 'a>>;
}

impl Storage for LocalDisk {
    fn put<'a>(&'a self, name: &'a str, data: Vec<u8>)
        -> Pin<Box<dyn Future<Output = Result<(), StorageError>> + Send + 'a>>
    {
        Box::pin(async move {
            tokio::fs::write(self.root.join(name), data).await.map_err(StorageError::Io)
        })
    }
}
```

Тип `Pin<Box<dyn Future<Output = T> + Send + 'a>>` настолько частый, что в крейте `futures` есть
псевдоним `BoxFuture<'a, T>`. Тело реализации оборачивается в `Box::pin(async move { .. })` либо в
`async move { .. }.boxed()` (метод `boxed` из `futures::FutureExt`). Посмотри `future_boxing`:
там видно, где компилятор проверяет `Send` и почему тип теперь один.

Цена такого решения: аллокация на каждый вызов. Для обращения к диску, базе или сети это ничто, а
в горячем цикле вызова на каждый байт уже заметно.

### Время жизни в BoxFuture

Future держит ссылки на свои аргументы, и это надо записать. Самая частая ошибка выглядит так:

```rust
trait Resolver: Send + Sync {
    fn lookup(&self, host: &str) -> BoxFuture<'_, Option<Ip>>;      // не соберётся в реализации
}
```

Правило пропуска времён жизни привязывает `'_` только к `&self`. Но future внутри захватывает ещё и
`host`, а компилятор не может доказать, что `host` проживёт столько же, сколько `self`. Нужен один
явный параметр на оба:

```rust
fn lookup<'a>(&'a self, host: &'a str) -> BoxFuture<'a, Option<Ip>>;
```

Читается так: «future не живёт дольше, чем и `self`, и `host`». Вызов с временной строкой после
этого собирается.

## Таблица обработчиков

Роутер, диспетчер команд, реестр вебхуков: везде нужна таблица «имя события, асинхронный обработчик».
Обработчики разные (функции, замыкания), а тип в таблице один. Его стирают в момент регистрации:

```rust
type Handler = Box<dyn Fn(Request) -> BoxFuture<'static, Reply> + Send + Sync>;

struct Router { routes: HashMap<&'static str, Handler> }
```

Метод `route` принимает любой подходящий обработчик: тип `F`, который по `Request` возвращает какой-то
future `Fut` с ответом `Reply`. В таблицу кладётся замыкание, которое вызывает `F` и сразу упаковывает
результат вызовом `.boxed()`. Разные обработчики дают разные `Fut`, а в таблице после упаковки они
все выглядят одинаково.

Откуда ограничения на `F` и `Fut`. `Box<dyn Fn + Send + Sync>` обещает, что таблицу можно делить между
потоками, поэтому и сам обработчик обязан быть `Send + Sync`. `'static` нужен, потому что он живёт
в таблице дольше вызова `route`. Future должен быть `Send` (иначе нельзя `spawn`) и `'static` (в типе
`BoxFuture<'static, ..>`). Обработчик, который что-то захватил (счётчик, `Arc` на состояние), клонирует
захваченное внутри замыкания, прежде чем отдать его в `async move`.

## Future и заимствования

`tokio::spawn` требует `'static`: задача может прожить сколько угодно, поэтому ссылаться на что-то
чужое ей нельзя. Типичная ошибка *borrowed data escapes outside of method* значит ровно это.

```rust
fn index_in_background(&self, paths: &[PathBuf]) -> JoinHandle<usize> {
    tokio::spawn(async move { self.index(paths).await })      // self и paths взяты взаймы
}
```

Всё, что нужно задаче, надо забрать с собой владельцем: общее состояние через `Arc::clone`, срезы через
`to_vec()`, `&str` через `to_string()`. Делать копии надо до `spawn`, снаружи блока `async move`.

## Повтор требует фабрики

Операцию, которую нужно выполнить дважды, нельзя передать готовым future: после завершения его не
дождаться второй раз. Передают замыкание, которое создаёт новый future при каждом вызове:

```rust
async fn with_timeout_retry<F, Fut, T>(mut make: F) -> Option<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = T>,
{
    for _ in 0..3 {
        if let Ok(value) = tokio::time::timeout(Duration::from_secs(1), make()).await {
            return Some(value);
        }
    }
    None
}
```

Замыкание живёт в вызывающем коде и спокойно может брать данные по ссылке:
`with_timeout_retry(|| client.fetch(&url))`.

## Асинхронные замыкания

Обычное замыкание с границей `F: Fn(&mut Conn) -> Fut` не может вернуть future, который
держит ссылку на свой аргумент: `Fut` один на все вызовы, а ссылка у каждого своя. Ради этого в Rust 1.85
появились асинхронные замыкания и трейты `AsyncFn`, `AsyncFnMut`, `AsyncFnOnce`:

```rust
async fn in_transaction<F, R>(db: &Db, work: F) -> R
where
    F: AsyncFnOnce(&mut Tx) -> R,
{
    let mut tx = db.begin().await;
    let result = work(&mut tx).await;
    tx.commit().await;
    result
}

let count = in_transaction(&db, async |tx| {
    tx.execute("INSERT ...").await;
    tx.count("users").await
}).await;
```

Выбор между тремя тот же, что у `FnOnce`, `FnMut`, `Fn`: вызывается один раз, много раз с
изменением состояния, много раз без. Ограничение: про `Send` для future такого замыкания трейт пока
ничего не обещает, поэтому в коде, который уходит в `spawn`, по-прежнему пригождаются обычные `Fn`
и `BoxFuture`.

## Рекурсия

`async fn` не может вызывать сама себя в таком виде: размер её future включал бы самого себя.

```rust
async fn dir_size(path: PathBuf) -> u64 {          // ошибка: recursion in an async fn requires boxing
    let mut total = 0;
    for child in children(&path).await {
        total += dir_size(child).await;
    }
    total
}
```

Рекурсивный вызов надо положить в коробку, чтобы размер стал конечным:

```rust
fn dir_size(path: PathBuf) -> BoxFuture<'static, u64> {
    Box::pin(async move { /* ... dir_size(child).await ... */ })
}
```

Цикл с `await` внутри обходит детей по одному. Нужно параллельно, тогда собери future детей и ждёшь их
вместе (`join_all`).

## Размер future

Задача tokio это один объект в куче, и его размер равен размеру состояния async-функции: всем
значениям, что живут через `.await`. Точнее, всем значениям, которые на момент какого-нибудь `.await`
ещё не вышли из области видимости. Если массив объявлен в функции и после него есть `.await`, место под
него в future зарезервировано, даже когда после ожидания массив уже не трогают. Большой массив на стеке внутри `async fn` раздувает каждую задачу;
`join!` из нескольких таких future складывает размеры.

```rust
async fn parse() {
    let buf = [0u8; 8192];       // лежит прямо в future
    read_into(&mut buf).await;
    use_it(&buf);
}
```

Проверить размер можно прямо в тесте: `std::mem::size_of_val(&parse())`. Лечится двумя способами. Положить
данные в кучу (`Vec`, `Box`): в future останется указатель. Или сузить область видимости блоком, чтобы
значение вышло из области видимости до `.await` (именно блоком: `drop(buf)` место в future не освобождает).

## Типичные ошибки

- `tokio::spawn` на методе обобщённого трейта с `async fn`: не `Send`. Выбери: `join!` без `spawn` или
  `fn .. -> impl Future + Send` в трейте.
- `Arc<dyn Trait>` над `async fn`: not dyn compatible. Нужен `BoxFuture`.
- Время жизни в `BoxFuture` привязано к `self`, а future держит ещё и аргумент: общий `'a` на оба.
- Охранник замка или `Rc` живёт через `.await`: future не `Send`. Узкий блок, а не `drop`.
- Рекурсивная `async fn` без `Box::pin`.
- Готовый future вместо фабрики там, где нужен повтор.
- Огромный массив на стеке в async-функции.
- Мысль «`Box<dyn Fn() -> Fut>`», где `Fut` один тип: у разных обработчиков типы future разные, нужен
  `BoxFuture`.

## Что почитать

- Блог Rust, «Announcing `async fn` and return-position `impl Trait` in traits» (выпуск Rust 1.75).
- Документация крейта `futures`: `BoxFuture`, `FutureExt::boxed`.
- Блог Rust, «Announcing Rust 1.85.0»: асинхронные замыкания и `AsyncFn`.
- Справочник Async Rust (rust-lang.github.io/async-book), главы про `Send` и закрепление.

## Задачи главы

Порт и обобщённый код (atr1). Что делать со `spawn` на методе трейта (atr2, atr3). `dyn` и время
жизни (atr4, atr5). Таблица обработчиков (atr6). Владение в задачах и повтор с фабрикой (atr7, atr8).
Асинхронные замыкания (atr9). Рекурсия (atr10). Проверка `Send` и размер future (atr11, atr12).
Проект `atr_ports` собирает порты соцсети, медленные подделки и таблицу команд.
