# Экзамен: планировщик заданий

Ночной сервис отчётов получает заказы на задания: собрать выгрузку, отправить письма, пересчитать
рейтинг. Заданий много, одновременно можно выполнять только несколько, одни важнее других, часть
иногда падает или зависает, а при остановке сервиса надо аккуратно довести начатое и ничего не
оставить после себя. Нужен планировщик, на который можно положиться. Подсказок нет, это экзамен по
всему разделу: задачи, время, синхронизация, каналы, `select!`, остановка и отладка.

## Что надо создать

Папка пуста, кроме приёмочного теста. Создай сам:

```
dbg_exam/
  Cargo.toml                    воркспейс (members = ["crates/jobq"])
  crates/
    jobq/
      Cargo.toml                твой крейт `jobq`
      src/lib.rs                решение
      tests/acceptance.rs       приёмочные тесты (уже лежат, менять нельзя)
```

В `Cargo.toml` крейта подключи `tokio` (для библиотеки хватит `sync`, `time`, `rt`, `macros`; для
тестов нужны `full` и `test-util`), `futures` (зависимости других крейтов не нужны).
Версии: `tokio = "1"`, `futures = "0.3"`. Редакция 2024.

## Открытый интерфейс

Тесты используют ровно это, имена и сигнатуры менять нельзя.

```rust
pub type JobId = u64;
pub type JobFn = Box<dyn Fn() -> BoxFuture<'static, Result<String, String>> + Send + Sync>;

pub struct JobSpec { pub name: String, pub priority: u8, pub timeout: Duration, pub run: JobFn }
pub struct Config  { pub workers: usize, pub retries: u32, pub backoff: Duration }   // Debug, Clone

pub enum Status { Queued, Running, Retrying(u32), Done(String), Failed(String), TimedOut, Cancelled }
//   Debug, Clone, PartialEq;  fn is_terminal(&self) -> bool  (Done, Failed, TimedOut, Cancelled)
pub struct Closed;                                   // Debug, PartialEq
pub struct Report { pub completed: usize, pub cancelled: usize }   // Debug, PartialEq

pub struct Scheduler;   // Clone
pub struct Runner;

impl Scheduler {
    pub fn start(config: Config) -> (Scheduler, Runner);
    pub async fn submit(&self, spec: JobSpec) -> Result<JobId, Closed>;
    pub async fn status(&self, id: JobId) -> Result<Option<Status>, Closed>;
    pub async fn cancel(&self, id: JobId) -> Result<bool, Closed>;
    pub async fn wait(&self, id: JobId) -> Result<Option<Status>, Closed>;
    pub fn updates(&self) -> BoxStream<'static, (JobId, Status)>;
}
impl Runner { pub async fn shutdown(self, grace: Duration) -> Report; }
```

`BoxFuture` и `BoxStream` это типы из крейта `futures` (`futures::future::BoxFuture`,
`futures::stream::BoxStream`). `JobFn` вызывается заново на каждую попытку.

## Требования

1. **Номера.** `submit` присваивает номера 1, 2, 3 по порядку подачи и сразу возвращает номер. Задание
   сначала `Queued`.
2. **Воркеры.** Одновременно выполняется не больше `config.workers` заданий. Освободившееся место
   занимает следующее задание сразу, в тот же момент.
3. **Приоритеты.** Следующим берётся задание с наибольшим `priority`, при равных то, что подано
   раньше. Уже начавшиеся задания не вытесняются.
4. **Таймаут.** Попытка, не уложившаяся в `spec.timeout`, прерывается, и задание получает `TimedOut`.
   Таймаут не повторяется.
5. **Повторы.** Если задание вернуло `Err(e)`, оно повторяется до `config.retries` раз. Перед `k`-м
   повтором пауза `backoff * 2^(k-1)`, то есть `backoff`, `2*backoff`, `4*backoff`. На это время задание
   сохраняет свой воркер. Состояния: `Running`, при ошибке `Retrying(k)`, после паузы снова `Running`.
   Повторы кончились, итог `Failed(последняя ошибка)`. Успех `Done(текст)`.
6. **Паника.** Паника внутри задания даёт `Failed("panic: <текст паники>")` без повторов. Планировщик
   продолжает работать.
7. **Отмена.** `cancel(id)`: задание в очереди получает `Cancelled` и больше не запускается;
   выполняющееся (`Running` или `Retrying`) прерывается (его future уничтожается), получает
   `Cancelled`, воркер освобождается сразу. Возвращает `true`, если отмена состоялась. Завершённое или
   неизвестное задание не отменяется: `false`.
8. **Ожидание.** `wait(id)` возвращает итоговое состояние задания (по любому исходу), сразу, если оно
   уже завершено. Неизвестный номер: `Ok(None)`. `status(id)` текущее состояние или `None`.
9. **Обновления.** `updates()` подписывает на поток всех изменений состояний (`(номер, состояние)`) в
   порядке, в котором они происходили: `Queued` при подаче, каждое `Running`, `Retrying`, итог.
   Подписчик видит только изменения после подписки. Отстающий подписчик не должен тормозить
   планировщик.
10. **Остановка.** `Runner::shutdown(grace)`: новые `submit` перестают приниматься (`Err(Closed)`); задания
    в очереди получают `Cancelled`; выполняющимся даётся до `grace` времени доработать; оставшиеся
    прерываются и получают `Cancelled`. Метод возвращается, когда выполняющихся нет (досрочно, если все
    закончились раньше срока). `Report.completed` число заданий в `Done`, `Failed` или `TimedOut`,
    `Report.cancelled` число `Cancelled`. После остановки методы `Scheduler` отвечают `Err(Closed)`.
11. **Чистота.** После остановки и выброса всех `Scheduler` в рантайме не остаётся ни одной задачи этого
    планировщика, а ни одна ошибка одного задания не должна влиять на другие.

## Критерии приёмки

`cargo test --workspace` в корне проекта, тесты в `crates/jobq/tests/acceptance.rs`. Время в них
замороженное (`start_paused`), поэтому проверяются точные моменты: волны заданий, паузы повторов,
остановка. Решение должно проходить `cargo clippy --workspace --all-targets -- -D warnings`.

## Запуск

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
