# Веха 62. Redis в деле

С m50 соцсеть работает несколькими экземплярами, а часть состояния всё ещё живёт в памяти каждого процесса. Лимиты m47 считаются отдельно на каждом шлюзе: два шлюза пропускают вдвое больше. Неудачные входы m49 забываются при перезапуске auth-service. Присутствие m41 держится на KV в NATS, кэш профилей m48 у каждого шлюза свой, и сто одновременных промахов кэша дают сто запросов в user-service. Каждый лайк в Postgres обновляет одну и ту же горячую строку поста.

Эта веха переносит общее быстрое состояние в Redis. resp-client получает Lua-скрипты (`EVALSHA` с запасным `EVAL` при `NOSCRIPT`). Шлюз считает лимиты пользователя и адреса ведром токенов на Lua, общим для всех экземпляров. auth-service считает неудачные входы `INCR` со сроком, держит активность сессии со скользящим сроком и множество сессий пользователя для «выйти на всех устройствах». Присутствие это `SET ... EX`, снимается скриптом только своим экземпляром, а онлайн друзей проверяется одним конвейером. Кэш профилей лежит в Redis: пересчёт под замком `SET NX PX`, разброс срока жизни, сброс по событию после коммита. Лайки копятся в хеше Redis и сбрасываются в Postgres пачкой через `RENAME` рабочего ключа.

Всё собирается из глав данных: 23_redis_streams и 24_redis_patterns (задачи rdx, rdx_notifications, rdp, rdp_gateway_limits, rdp_profile_cache).

## Что сделать

1. **resp-client**: `script/`: `Script::new(lua)`, `Client::load` (`SCRIPT LOAD`), `Client::eval` (`EVALSHA`, при `NOSCRIPT` целиком `EVAL` и повторная загрузка); `Value: Default`.

2. **Шлюз** `common/shared.rs`: `SharedLimits` (ведро на Lua по времени Redis `TIME`, ключи `rate-user` и `rate-ip`, недоступный Redis пропускает), `RedisPresence` (`touch` это `SET EX`, `release` скриптом «удалить, если моё», `online`, `online_many` одним конвейером), `RedisProfiles` (`cached` через `MGET`, `claim` через `SET NX PX`, `store` со сроком `spread`, `invalidate`); `Guards::with_redis`, `take_user` и `take_ip` (общие или местные вёдра); лимит пользователя в WS проверяется в задаче запроса; `Realtime` пишет присутствие в Redis; лента берёт авторов из общего кэша; `users.online_friends`; `GATEWAY_REDIS_URL` или `RUSTLINGS_REDIS_URL`.

3. **auth-service**: порт `service/activity.rs` (`Activity`: `strikes`, `failed`, `cleared`, `opened`, `touched`, `closed_all`; `SESSION_IDLE`), адаптер `client/activity.rs` (`RedisActivity`); вход считает задержку по счётчикам Redis и открывает активность сессии; `validate` продлевает её и отказывает простоявшей сессии; `logout_all` снимает все; `Throttle::after(count)`.

4. **post-service**: `repository/likes.rs` (порт `PendingLikes`, `PgPosts::with_pending`, `apply_likes` одной командой через `unnest`), адаптер `client/likes/` (`RedisLikes`: `HINCRBY` в рабочий хеш, `flush`: подбор брошенных рабочих ключей и `RENAME` текущего, `HGETALL`, сброс, `DEL`), фоновый сброс каждые `LIKE_FLUSH_EVERY`.

5. **CLI**: `online` без ника показывает друзей в сети.

## Приёмка

Тестам нужны Postgres, Scylla, Redis и NATS курса.

- `resp-client/tests/journey/m62_redis_patterns.rs`: скрипт по sha, после `SCRIPT FLUSH` через `EVAL`, затем снова по sha.
- `api-gateway/tests/journey/m62_redis_patterns.rs`: два шлюза делят одно ведро, чужое присутствие не снимается, онлайн одним конвейером, замок кэша один, запись снимает замок, сброс по событию.
- `auth-service/tests/journey/m62_redis_patterns.rs`: счётчик неудачных входов, сброс после успеха, скользящий срок сессии, выход на всех устройствах.
- `post-service/tests/journey/m62_redis_patterns.rs`: лайки копятся, счётчик в Postgres ждёт, сброс через `RENAME` подбирает и брошенный рабочий ключ.
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: нет `TokenBucket` в шлюзе; `EVALSHA` и `NOSCRIPT`; ведро на Lua; `SET EX`; `online_many`; замок `NX PX`; разброс срока; `INCR`; `SMEMBERS`; `RENAME`; и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

Подними два шлюза (`devctl` из m50) с одним Redis: частые запросы получают отказ лимитера на любом из них. alice и bob друзья, `online` у alice показывает bob, пока тот в сети.
