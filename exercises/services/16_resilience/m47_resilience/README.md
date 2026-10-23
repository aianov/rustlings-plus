# Веха 47. Переживаем соседей

Нагрузка из m46 показала, сколько соцсеть держит, пока все живы. Но в жизни соседи падают: user-service перезапускается, post-service тормозит, клиент с плохой связью жмёт «опубликовать» трижды. Сейчас каждый отказ соседа превращается в ошибку пользователю, повторы в клиентах легко устраивают шторм на только что поднявшийся сервис, один слишком активный пользователь съедает шлюз, а пост, отправленный дважды из-за обрыва связи, появляется в ленте дважды.

Эта веха учит соцсеть переживать соседей. В service-runtime появляется набор приёмов: `Deadline` с остатком бюджета вниз по цепочке, повторы только нужных кодов с полным jitter и общим бюджетом повторов, предохранитель с одной пробой в half-open, переборка на зависимость и вёдра жетонов с выбросом простаивающих. Шлюз ограничивает каждого пользователя и каждый адрес, при перегрузке сразу отвечает 503 с `Retry-After`, а лента при отказе user-service открывается с профилями авторов из устаревшего кэша и пометкой. У `posts.create` появляется ключ идемпотентности: повтор отдаёт сохранённый ответ, а два одновременных запроса с одним ключом дают один пост.

Всё собирается из главы 16_resilience: задачи с svres1 по svres16 и проект svres_quiz.

## Что сделать

1. **service-runtime `resilience/`** (`mod.rs` и файлы `deadline.rs`, `retry.rs`, `breaker.rs`, `bulkhead.rs`, `buckets.rs`):
   - `Deadline` (`at`, `within`, `instant`, `left`, `expired`, `reserve(своя работа)`, `cap(бюджет)`);
   - `RetryPolicy { attempts, base, cap }` (по умолчанию 3, 20 мс, 500 мс), `full_jitter(policy, номер повтора, случайное)` от нуля до `min(cap, base * 2^n)`, `RetryBudget::new(доля, потолок)` (`deposit`, `withdraw`, `balance`), `retry(policy, budget, deadline, retryable, op)`: каждый вызов пополняет бюджет, повтор только для `retryable`, только пока есть попытки и жетон, и только если до дедлайна больше паузы;
   - `Breaker::new(порог, остывание)` с состояниями `BreakerState { Closed { failures }, Open { until }, HalfOpen { probing } }`: `allow() -> Result<Pass, Rejected { retry_after }>`, после порога подряд отказов размыкается, после остывания пускает ровно одну пробу, успех замыкает, отказ снова размыкает; `Pass::success`, `Pass::failure`, брошенный `Pass` это отказ;
   - `Bulkhead::new(n)`: `try_enter() -> Result<Seat, Busy>` отказывает сразу, `in_use`;
   - `TokenBucket` и `Buckets<K>::new(в секунду, ёмкость)`: `take(key)` и `take_at(key, now)` отвечают `Err(сколько ждать)`, `evict_idle(простой, now)` выбрасывает полные вёдра, которых не трогали дольше простоя, `len`.

2. **post-service**: поле `idempotency_key = 5` у `CreatePostRequest`; `service/idempotency.rs`: `Idempotency` хранит до `KEPT_KEYS` = 10 000 ключей на пару (автор, ключ) в `OnceCell`, `good_key` (до `MAX_KEY` = 64 печатных ASCII); `PostService::create_once(users, author, key, draft, origin)` через `get_or_try_init`: успех запоминается, отказ нет (повтор после отказа создаёт заново); кривой ключ это `InvalidArgument`.

3. **contracts**: у `FeedPage` поля `authors: Vec<UserView>` и `stale: bool` (оба необязательные в JSON), `FeedPage: Default`.

4. **Шлюз**:
   - `common/guards.rs`: `RateLimits { user_rate, user_burst, ip_rate, ip_burst, shed_inflight }` (по умолчанию 100 и 200 в секунду на пользователя, 2000 и 4000 на адрес, 4096 запросов в полёте) из `GATEWAY_USER_RATE`, `GATEWAY_USER_BURST`, `GATEWAY_IP_RATE`, `GATEWAY_IP_BURST`, `GATEWAY_SHED_INFLIGHT`; `ProfileCache` (свежесть `GATEWAY_PROFILE_FRESH_MS`, 30 с; `fresh`, `stale`, `put`); `Guards` (вёдра пользователей и адресов, кэш профилей, предохранитель, переборка на 64 вызова и бюджет повторов к user-service; `admit()` считает запросы в полёте и отказывает сверх предела); фоновая задача раз в минуту выбрасывает тихие вёдра;
   - `controller/limits.rs`: слой `guard_requests` после `trace_requests`: `/health` проходит всегда, сверх предела в полёте быстрый 503 с `Retry-After: 1`, пустое ведро адреса это 429 с `Retry-After`; лимит на accept остаётся прежним (`GATEWAY_MAX_CONNECTIONS`);
   - запрос WebSocket сначала берёт жетон из ведра пользователя, иначе ответ `exhausted` со сроком;
   - `routes/profiles.rs`: `posts.feed` добавляет профили авторов страницы: свежие из кэша, остальные из user-service под переборкой, предохранителем и повторами `Unavailable` и `DeadlineExceeded` в пределах бюджета части ответа; несуществующий автор просто пропускается; при отказе берётся устаревший профиль и `stale: true`;
   - `posts.create` передаёт `idempotency_key` из данных запроса.

Клиент курса приходит готовым: `post` шлёт свой ключ идемпотентности и повторяет запрос с тем же ключом, если связь оборвалась, а лента с `stale` начинается строкой «профили могут быть устаревшими».

## Дерево после вехи

```
backend/src/
  crates/service-runtime/src/resilience/
    mod.rs
    deadline.rs
    retry.rs
    breaker.rs
    bulkhead.rs
    buckets.rs
  microservices/api-gateway/src/
    common/guards.rs
    controller/limits.rs
    routes/profiles.rs
  microservices/post-service/src/service/idempotency.rs
```

Остальное дерево прежнее.

## Приёмка

Тестам нужен NATS курса с JetStream на `127.0.0.1:54222` (адрес из `RUSTLINGS_NATS_URL`).

- `service-runtime/tests/journey/m47_resilience.rs` (время на паузе): дедлайн и резерв; полный jitter под потолком; повтор только нужного; бюджет гасит шторм повторов; повторы не переживают дедлайн; предохранитель с одной пробой; переборка; вёдра с притоком и выбросом.
- `api-gateway/tests/journey/m47_resilience.rs`: один ключ это один пост даже для двух одновременных запросов с разных устройств; ведро пользователя; ведро адреса и `/health` без жетонов; сброс 503 с `Retry-After`; лента без user-service открывается с устаревшим профилем и пометкой.
- `contracts` m13 (у `FeedPage` новые поля).
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву; полный jitter, бюджет повторов, дедлайн у повтора, одна проба в half-open, брошенный пропуск как отказ, переборка без очереди, выброс тихих вёдер, 503 с `Retry-After`, ведро адреса и пользователя, устаревший профиль, `get_or_try_init` у ключа идемпотентности, поле в proto; и все договорённости прошлых вех.

## Как запустить

NATS курса должен работать: `127.0.0.1:54222`.

```
cd backend
cargo make check
cargo make demo
```

В клиенте alice: `register alice correct-horse`, `login alice correct-horse`, `post привет`. Во втором клиенте bob подписывается на alice и смотрит `feed`. Останови user-service (Ctrl-C в его окне или `kill`): `feed` у bob по-прежнему открывается, первой строкой «профили могут быть устаревшими». Повтор создания поста с тем же ключом (клиент делает его сам при обрыве связи) не плодит дублей.
