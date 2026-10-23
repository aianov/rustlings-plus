# Веха 61. Свой клиент Redis

Соцсети нужны быстрые и неточные вещи, которым не место в Postgres: сколько разных людей посмотрело пост, кто заходил на твой профиль, какие теги сейчас в ходу. Это счётчики и короткие списки со сроком жизни, ровно то, для чего есть Redis. Готовый клиент мы не берём: как и движки HTTP и NATS, клиент Redis пишется своими руками поверх своего же `framing`.

Эта веха строит крейт `resp-client`: кодек RESP на трейтах `Decoder` и `Encoder` из `framing` с пределами на длину строки, число элементов и глубину; соединение-актор, который принимает запросы из канала, пишет их подряд и раздаёт ответы по очереди ответчиков; конвейер (`pipeline`), типизированные команды, таймаут на запрос без рассинхрона (опоздавший ответ выбрасывается, а не достаётся следующей команде), переподключение после обрыва и отдельные соединения для блокирующих команд. Сервисы используют его для трёх вещей: уникальные просмотры постов через HyperLogLog, «недавно смотрели твой профиль» через `LPUSH` и `LTRIM` со сроком, популярные теги через sorted set по часам. Ключи строятся по схеме `social:<сервис>:<вид>:<id>` и перебираются `SCAN`, а не `KEYS`.

Всё собирается из глав данных: 20..22 (задачи resp, resp_codec, rcl, rcl_resp_client, rds, rds_profile_types) и задачи nfrm12.

## Что сделать

1. **Крейт `src/crates/resp-client`** (tokio, bytes, framing):
   - `value/`: `Value { Simple, Error, Integer, Bulk(Option<Bytes>), Array(Option<Vec<Value>>) }`, `as_int`, `as_text`, `items`;
   - `codec/`: `RespCodec { max_bulk, max_items, max_depth }` (`Decoder` ждёт целый кадр, `Encoder<&Cmd>` пишет массив строк), `RespError` (`TooLarge`, `TooDeep`, `BadFrame`, `Server`, `Timeout`, `Closed`, `Io`);
   - `command/`: `cmd("SET").arg(..).args(..)`;
   - `conn/`: актор на `into_split`: читатель `Framed` и писатель, очередь `VecDeque<Waiting>`, ленивое (пере)подключение, обрыв отдаёт всем ждущим `Closed`;
   - `client/`: `Client::connect` (клон это ручка к тому же актору), `pipeline`, `query` (ошибка Redis это `Server`), `dedicated` (отдельное соединение, например для `BLPOP`), типизированные `ping`, `get`, `set_ex`, `incr`, `del`, `expire`, `pfadd`, `pfcount`, `lrange`, `zrevrange_scores`, `blpop`;
   - `config/`: `RedisConfig` из `RUSTLINGS_REDIS_URL` (по умолчанию 127.0.0.1:56379), таймаут запроса;
   - `keys/`: `key(сервис, вид, id)`, `scan_keys(client, шаблон, пачка)`.

2. **post-service**: порт `service/heat.rs` (`Heat`: `view`, `views`, `tagged`, `top_tags`), адаптер `client/heat/` (`RedisHeat`: `PFADD` и `PFCOUNT` с `EXPIRE`, `ZINCRBY` в ключ часа, `ZUNIONSTORE` за 24 часа и `ZREVRANGE WITHSCORES`); `Post.views = 10`, rpc `TopTags`; `POSTS_REDIS_URL` или `RUSTLINGS_REDIS_URL` включает Redis; тренды переехали в `service/text.rs` (лимит 300 строк).

3. **user-service**: порт `service/guests.rs` (`Guests`), адаптер `client/guests/` (`LREM`, `LPUSH`, `LTRIM 0 9`, `EXPIRE` неделя одним конвейером); `GetProfile` записывает гостя; rpc `Guests`; `client/relay.rs` стал `client/relay/` (правило одинаковых модулей).

4. **Шлюз и CLI**: `PostView.views`, `posts.tags`, `users.guests`; команды `guests` и `tags`, у поста «просмотров N».

## Приёмка

Тестам нужны Postgres, Scylla, Redis (`docker start rustlings-redis`, 127.0.0.1:56379) и NATS курса.

- `resp-client/tests/journey/m61_redis_client.rs`: кодек ждёт целый кадр на любом разрезе, пределы и кривые кадры отбиваются, команда кодируется; конвейер отвечает по порядку, ошибка Redis приходит `Server`, опоздавший ответ `BLPOP` не достаётся `PING`, блокирующая команда на своём соединении, клиент переподключается после `CLIENT KILL`, `SCAN` проходит все страницы.
- `post-service/tests/journey/m61_redis_client.rs`: уникальные просмотры, теги суммируются по часам за сутки.
- `user-service/tests/journey/m61_redis_client.rs`: гостей десять, повторный гость поднимается без дубля.
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: `resp-client` в воркспейсе, только на tokio, bytes и framing; redis, fred и deadpool-redis запрещены; без комментариев; нет `"KEYS"` в сервисах; кодек на трейтах framing с пределами; очередь ответчиков; схема ключей; `PFADD`, `ZINCRBY`, `LTRIM`; и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

bob открывает пост alice (`show 1`), и у поста растёт «просмотров». bob смотрит `card alice`, а alice в `guests` видит bob. Посты с тегами наполняют `tags`.
