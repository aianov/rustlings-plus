# Веха 42. Фоновые задачи

У соцсети накопились дела, которые не помещаются в обработчик запроса. Истёкшие сессии чистит каждый экземпляр auth-service по своему таймеру, и после деплоя три экземпляра делают одну и ту же чистку в одну секунду. Ежедневной сводки «у тебя N непрочитанных» нет вовсе. А просьба «удали мой аккаунт» сейчас невыполнима: аккаунт в auth-service, профиль и подписки в user-service, посты в post-service, лента в feed-service. Общей транзакции у них нет, а у рекордсмена тысячи постов.

Эта веха даёт соцсети фоновые задачи. Крейт `jobs` держит очередь на JetStream (stream с удержанием WorkQueue): задача подтверждается после работы, временная ошибка повторяется с растущей паузой, постоянная уходит в мёртвую очередь с причиной, отложенная ждёт своего срока, а остановка воркера сразу возвращает набранное. Периодические дела делает один экземпляр на период: лидер берёт аренду в KV с номером. Длинная рассылка идёт с контрольной точкой и после сбоя продолжается с места остановки. Удаление аккаунта это сага: заморозка, скрытие профиля, посты пачками, профиль, учётная запись, а при отказе обратимого шага сделанное откатывается в обратном порядке.

Всё собирается из главы 10_jobs: задачи с svjob1 по svjob12 и проект svjob_quiz (там очередь и аренда в памяти на логическом времени), плюс сага из svevt16 и svevt17. Здесь те же правила работают на настоящем JetStream.

## Что сделать

1. **nats-engine**:
   - `StreamSpec::work_queue()`: stream с удержанием WorkQueue (запись исчезает после подтверждения);
   - `js/pull.rs`: `puller::<T>(&bus, &PullSpec { stream, durable, filter, ack_wait })` и `Puller::take(max, wait) -> Vec<Taken<T>>`. У выданного сообщения `delivered`, `published_ms` (когда брокер его записал), `item` и действия `ack()` (с подтверждением брокера), `nak(delay)` (ноль значит сразу), `progress()`, `bury(dead, reason)` (мёртвое письмо и `Term`);
   - `Bucket`: `put`, `get` (удалённый или истёкший ключ это `None`), `claim(key, value) -> Option<номер>` (только если ключа нет), `renew(key, value, номер) -> Option<новый номер>` (только со своим номером), `release(key, номер) -> bool` (чужую запись не трогает).

2. **Крейт `src/crates/jobs`** (зависимости только nats-engine, tokio, serde, serde_json, futures; async-nats тестам):
   - `job/`: `Job { id, kind, payload, run_at }` (`Job::new`, `at(run_at)`), `Failure { Transient, Permanent }`, `Outcome`, `now_ms()`;
   - `policy/`: `Policy { first, cap, attempts }` (по умолчанию 1 с, 60 с, `MAX_ATTEMPTS` = 5), `backoff(attempt)` (вдвое от `first`, не больше `cap`, без переполнения), `decide(outcome, attempt) -> Decision { Done, Retry(пауза), Dead(причина) }`: исчерпанные попытки это `Dead` с числом попыток и последней ошибкой;
   - `queue/`: `QueueSpec { stream, prefix, dead_stream, dead_prefix }`, `Queue::open` заводит WorkQueue-stream `<prefix>.>` и обычный stream мёртвых `<dead_prefix>.>`; `enqueue(&job)` публикует в `<prefix>.<kind>` с `Nats-Msg-Id = job.id`, поэтому повторная постановка это одна задача; вид задачи это одно слово темы;
   - `worker/`: `WorkerSpec { durable, kind, batch, ack_wait, policy }`, `Worker::start(&queue, spec, handler)`. Воркер берёт пачку и обрабатывает по одной. Нечитаемый конверт уходит в мёртвую тему `unreadable`. Задача раньше `run_at` возвращается одним `nak` на всё оставшееся время, обработчик не вызывается. Попытка это `delivered`, минус одна выдача, если задача пришла раньше срока (`run_at > published_ms`). Пока обработчик работает, брокеру идёт `progress` раз в треть `ack_wait`. Итог по `decide`: `ack`, `nak` с паузой или `bury` с причиной (не записалось мёртвое письмо, значит `nak`). `stop()`: текущая задача и набранные, но не начатые, возвращаются `nak` без паузы; уже начатый fetch не бросается, чтобы не потерять выданное;
   - `periodic/`: `spread(instance, every)` (устойчивый сдвиг из имени), `Periodic::new(instance, every, now)` (первое срабатывание на сетке со сдвигом, строго после `now`), `due(now)` (не больше одного срабатывания, следующее на прежней сетке строго после `now`); `Leader::new(name, owner, every, leases, now)` и `tick(now, run)`: при срабатывании берёт аренду `<name>.<now / every>` и только тогда вызывает `run(period)`;
   - `lease/`: `Leases::open(&bus, bucket, ttl)` (бакет со сроком жизни записи), `acquire(name, owner) -> Option<Lease { name, owner, revision }>`, `renew(&lease) -> Option<Lease>` (номер растёт), `release(&lease) -> bool`; `run_exclusive(&leases, lease, work)` продлевает аренду раз в треть срока и сразу останавливает работу с `LeaseLost`, если продлить не вышло;
   - `checkpoint/`: `Checkpoints::open(&bus, bucket)` (`get`, `set`), `run_checkpointed(job, items, checkpoints, send)`: элементы по возрастанию, пройденные пропускаются, `send(ключ "<job>:<элемент>", элемент)`, точка пишется сразу после каждой отправки, ошибка обрывает работу;
   - `saga/`: трейт `Step { name, forward_only, run, compensate }`, `SagaLog` (журнал `Started`, `Done`, `Aborted`, `Compensated` в KV под идентификатором саги), `Saga::new(id, &log).step(..).run()`. Сделанные шаги по журналу пропускаются. Отказ обратимого шага пишет `Aborted` и откатывает начатые шаги в обратном порядке, упавший не откатывается; каждая компенсация до трёх попыток, неоткатившиеся перечислены в `RolledBack { stuck }`. После необратимого шага (`forward_only`) отказ это `Retry`: сага продолжится при повторе задачи. Перезапуск с `Aborted` в журнале доделывает откат.

3. **contracts**: темы `jobs_stream()` (`<КОРЕНЬ>_JOBS`), `jobs()`, `jobs_dead_stream()`, `jobs_dead()`, бакеты `leases()`, `checkpoints()`, `sagas()`; факт `UserDeleted { user_id }` (`user.deleted`, `KINDS` теперь 11); `NoticeKind::Digest`, у `NoticeView` поле `count` (старые уведомления без него читаются), `Push::Digest { to, unread }`.

4. **auth-service**:
   - чистка сессий: с NATS это `Leader` «sweep» с арендой в `<LEASES>_SWEEP` (`AUTH_SWEEP_EVERY_MS`, 60 с); без NATS прежний таймер;
   - `POST /auth/delete-account` (Bearer): `request_deletion` проверяет токен и ставит задачу `delete_account` с идентификатором саги `delete.<user>.<секунда>`, ответ 202 сразу; без NATS это 503;
   - воркер `auth-delete` ведёт сагу (`service/deletion.rs`): `freeze` (заморозить учётку и погасить все её сессии с объявлением шлюзам; компенсация размораживает), `hide` (скрыть профиль; компенсация возвращает), `posts` (необратимо: страницы по 50 через `AuthorPosts`, удаление с контрольной точкой от имени пользователя), `profile` (стереть профиль), `account` (стереть учётку). Порты `ProfileKeeper`, `PostEraser`, `DeletionQueue`, адаптеры в `client/saga.rs` (временные коды gRPC это `Transient`, остальные `Permanent`, пост, которого уже нет, не ошибка). Откат это постоянная ошибка задачи: она ляжет в мёртвую очередь с причиной;
   - у учётки флаги `frozen` и `deleted`: замороженная не входит, удалённая исчезает из памяти и после перезапуска; конфиг `POSTS_GRPC_ADDR`.

5. **user-service**: RPC `HideProfile`, `ShowProfile`, `DeleteProfile` (`OwnProfileRequest {}`, действует x-user-id, повтор не ошибка). Скрытый профиль не находится, не принимает подписок и не виден в списке; удаление стирает профиль, подписки и скрытия, освобождает ник и пишет `user.deleted` в outbox. Состояние вынесено в `repository/state.rs`, жизненный цикл в `repository/lifecycle.rs` (`LifecycleRepository`), `UserService::with_lifecycle`, `Fate { Hide, Show, Erase }`.

6. **post-service** пускает и auth-service (`Gate::only(&[GATEWAY, AUTH])`).

7. **feed-service**: `user.deleted` убирает автора, его подписки, подписки на него и его посты (`Change::Gone`).

8. **notify-service**: лидер «digest» (`NOTIFY_DIGEST_EVERY_MS`, сутки) ставит задачу `digest.<период>`, воркер `notify-digest` рассылает сводку всем с непрочитанным с контрольной точкой; сводка это уведомление `digest` с числом непрочитанных, и пока одна не прочитана, вторая не появляется.

9. **Окружение**: `AUTH_SWEEP_EVERY_MS` и `NOTIFY_DIGEST_EVERY_MS` в `.env.example` и `.env`.

Клиент курса приходит готовым: команда `delete account`, пуш «* сводка: непрочитанных N, загляни в bell», сводка в `bell`.

## Дерево после вехи

```
backend/src/
  crates/jobs/
    Cargo.toml
    src/
      lib.rs
      job/mod.rs          Job, Failure, Outcome
      policy/mod.rs       Policy, Decision
      queue/mod.rs        Queue, QueueSpec
      worker/mod.rs       Worker, WorkerSpec, Handler
      periodic/mod.rs     Periodic, Leader, spread
      lease/mod.rs        Leases, Lease, run_exclusive
      checkpoint/mod.rs   Checkpoints, run_checkpointed
      saga/mod.rs         Saga, Step, SagaLog
  crates/nats-engine/src/js/pull.rs
  microservices/auth-service/src/
    service/deletion.rs   сага удаления и её порты
    client/saga.rs        профили, посты и постановка задачи
    controller/jobs.rs    воркер удаления и лидер чистки
  microservices/user-service/src/repository/
    state.rs
    lifecycle.rs
  microservices/notify-service/src/controller/jobs.rs
```

Остальное дерево прежнее.

## Приёмка

Тестам нужен NATS курса с JetStream на `127.0.0.1:54222` (адрес из `RUSTLINGS_NATS_URL`).

- `jobs/tests/journey/m42_jobs.rs`: пауза и потолок, безнадёжные задачи; повторы с растущей паузой, мёртвая очередь с причиной, один id это одна задача; отложенная задача не раньше срока и не мешает остальным; остановленный воркер сразу отдаёт задачи соседу; одно срабатывание после паузы и разброс экземпляров; аренда с владельцем, номером, сроком и перехватом; три экземпляра делают одну чистку на период; рассылка продолжается после сбоя без двойных писем; сага откатывается в обратном порядке, после необратимого шага повторяется и продолжает с недоделанного.
- `auth-service/tests/journey/m42_jobs.rs`: удаление идёт auth, user, посты, профиль, после сбоя посреди постов продолжается без повторов, замороженный не входит, удалённого нет; отказ скрытия откатывает заморозку.
- `user-service/tests/journey/m42_jobs.rs`: скрытый профиль недоступен до возврата, стёртый навсегда, `user.deleted` в outbox, перезапуск его не воскрешает, ник освобождается.
- `feed-service/tests/journey/m42_jobs.rs`: удалённый пользователь пропадает из всех лент.
- `notify-service/tests/journey/m42_jobs.rs`: сводка считает непрочитанное и не повторяется; два экземпляра за несколько периодов дают одну сводку.
- `api-gateway/tests/journey/m42_jobs.rs`: `delete account` в клиенте курса закрывает сокет, посты пропадают из ленты подписчика, профиля и входа больше нет; сводка приходит пушем.
- `contracts/tests/journey/m42_jobs.rs`: имена stream'ов и бакетов, факт `user.deleted`, уведомление и пуш сводки.
- новые версии тестов прошлых вех: m38 contracts и шлюза (у `NoticeView` есть `count`), m41 contracts (типов событий больше десяти), m38 и m39 notify-service (тесты удаляют и очередь задач с бакетами), m32 шлюза (auth-service теперь пускают в post-service, чужаки user-service, chat-service и notify-service), `fakes.rs` (три новых RPC user-service), `support.rs` (адрес post-service у auth-service, `with_digest`, очистка stream'ов и бакетов задач).
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву; `jobs` в воркспейсе и зависит только от разрешённого, `async_nats` в нём нет; WorkQueue, `Nats-Msg-Id = job.id`, потолок паузы, `nak` на остаток до `run_at`, `nak(Duration::ZERO)` при остановке, одно срабатывание после паузы, ревизия аренды, `run_exclusive`, контрольная точка после каждой отправки, обратный порядок компенсаций; шаги саги удаления и `run_checkpointed` в ней; лидеры `sweep` и `digest`; `user.deleted` от user-service к feed-service; post-service пускает auth-service; и все договорённости прошлых вех.

## Как запустить

NATS курса должен работать: `127.0.0.1:54222`.

```
cd backend
cargo make check
NOTIFY_DIGEST_EVERY_MS=20000 cargo make demo
```

В клиенте alice: `register alice correct-horse`, `login alice correct-horse`, `post первый`, `post второй`. Во втором клиенте bob: `register bob correct-horse`, `login bob correct-horse`, `follow alice`, `feed` (два поста alice). У alice через двадцать секунд придёт «* сводка: непрочитанных 1, загляни в bell» (bob подписался), её видно и в `bell`. Теперь alice: `delete account`. Клиент ответит «аккаунт удаляется», сокет alice закроется. Через пару секунд у bob `feed` пуст, `find alice` говорит «нет пользователя», а `login alice correct-horse` в любом клиенте отвечает «неверный ник или пароль».
