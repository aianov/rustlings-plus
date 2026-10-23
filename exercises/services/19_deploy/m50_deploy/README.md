# Веха 50. Вся соцсеть одной командой

Чтобы поднять соцсеть, `cargo make demo` запускает восемь процессов в фоне оболочки и надеется, что они успеют стартовать в нужном порядке. Если шлюз поднялся раньше user-service, он полминуты отвечает «жду сервисы». Остановка по Ctrl-C иногда оставляет процесс висеть на порту, и следующий запуск падает с «адрес занят». Перезапустить post-service, не уронив запросы клиентов, нельзя: экземпляр один. Секрет подписи ссылок лежит в `.env` рядом с кодом. А поднять то же самое на другой машине можно только руками.

Эта веха поднимает соцсеть одной командой. `tools/devctl` знает зависимости сервисов: `up` поднимает их по порядку и ждёт `/ready` каждого, `down` гасит в обратном порядке SIGTERM, ждёт дедлайн и только потом SIGKILL, `restart` перезапускает экземпляры по одному, выводя каждый из ротации до остановки. Если подъём сорвался, поднятое гасится: сирот не остаётся. post-service работает в двух экземплярах, и шлюз ходит в них по кругу, пропуская нездоровые. Секреты читаются из `*_FILE`. Для другой машины есть Dockerfile на каждый сервис (многоступенчатый, с кэшем зависимостей, release, не от root, exec-форма, свой healthcheck-бинарь) и `docker-compose.yml` всего стека с NATS и ожиданием готовности.

Всё собирается из главы 19_deploy: задачи с svdep1 по svdep8, svdep_docker и проект svdep_quiz, плюс tool3.

## Что сделать

1. **service-runtime**: `Source::resolve_files()` (вызывается в `Source::load`): для каждого `<КЛЮЧ>_FILE` значение `<КЛЮЧ>` читается из файла без перевода строки в конце; заданы оба это ошибка конфигурации, файла нет это `Problem::Unreadable`; `FILE_SUFFIX`.

2. **`tools/healthcheck`** (без зависимостей): `probe(url, limit) -> Result<u16, ProbeError { Url, Connect, Io, Status }>`: только `http://хост:порт/путь`, `GET` по `std::net`, успех только 200; бинарь `healthcheck <url>` отвечает кодом выхода (это `HEALTHCHECK` образов).

3. **`tools/devctl`**:
   - `plan/`: `Unit { name, service, program, args, env, ready, after }` (строитель `Unit::new(..).arg(..).env(..).ready(..).after(..)`), `Plan::with`, `Plan::order()` (сначала все экземпляры сервиса, от которого зависят; неизвестная зависимость и круг это ошибка); `social(source, bin)`: user-service, `DEVCTL_POSTS_REPLICAS` экземпляров post-service (по умолчанию 2, порты и admin со сдвигом на 10, `POSTS_NODE` свой), auth-service, chat-service, feed-service, notify-service и два шлюза (`api-gateway-1`, `api-gateway-2` на 7890) со списком реплик в `GATEWAY_POSTS_REPLICAS`;
   - `process/`: `signal(pid, имя)` и `alive(pid)` через `kill`, `Stopped { Gone, Terminated, Killed }`;
   - `control/`: `Devctl::new(папка состояния)` (`grace`, `ready_within`), `start` (pid-файл и лог в `<папка>/logs`), `wait_ready` (опрос `/ready`; умерший процесс это ошибка с хвостом лога), `stop` (SIGTERM, ожидание `STOP_GRACE`, SIGKILL), `up` (по порядку с ожиданием готовности, при сбое гасит поднятое в обратном порядке), `down` (в обратном порядке), `restart(service)` (экземпляры по одному: остановка, запуск, готовность), `status`;
   - бинарь `devctl up | down | restart <сервис> | status [--bin target/debug] [--state .devctl]`.

4. **Шлюз**: `client/replicas.rs`: `Replicas::connect("a,b")`, `supervise` (у каждой реплики своё наблюдение за здоровьем gRPC), `pick` (по кругу среди здоровых; если здоровых нет, по кругу среди всех), `healthy`; `GATEWAY_POSTS_REPLICAS` перекрывает `POSTS_GRPC_ADDR`; `/health` шлюза считает post-service готовым, если жива хоть одна реплика.

5. **Docker**: `src/microservices/<сервис>/Dockerfile` для всех семи: `FROM rust:1.88-bookworm AS build` с кэшем реестра и `target` (`--mount=type=cache`), `cargo build --release --locked -p <сервис> -p healthcheck`, `FROM debian:bookworm-slim AS runtime`, пользователь 10001, адреса `0.0.0.0`, `CERTS_DIR=/certs`, `HEALTHCHECK` через `healthcheck`, `ENTRYPOINT [...]`; `.env` в образ не копируется (`.dockerignore`); `docker-compose.yml`: NATS с JetStream и проверкой `/healthz`, сервисы с `depends_on: condition: service_healthy`, две реплики post-service, сертификаты томом, секрет `link_key` для шлюза через `GATEWAY_LINK_KEY_FILE`.

6. **Makefile.toml**: `cargo make up`, `cargo make down`, `demo` поднимает через devctl и гасит через `devctl down` при выходе; `.gitignore` с `secrets/`.

## Дерево после вехи

```
backend/
  docker-compose.yml
  .dockerignore
  src/microservices/*/Dockerfile
  src/microservices/api-gateway/src/client/replicas.rs
  tools/devctl/
    Cargo.toml
    src/
      lib.rs
      main.rs
      plan/mod.rs
      process/mod.rs
      control/mod.rs
  tools/healthcheck/
    Cargo.toml
    src/
      lib.rs
      main.rs
      probe/mod.rs
```

Остальное дерево прежнее.

## Приёмка

Тестам нужен NATS курса с JetStream на `127.0.0.1:54222` (адрес из `RUSTLINGS_NATS_URL`).

- `service-runtime/tests/journey/m50_deploy.rs`: секрет из файла, и значение, и файл сразу это ошибка, файла нет.
- `healthcheck/tests/journey/m50_deploy.rs`: 200 это готов, 503, мусор, чужая схема и закрытый порт нет.
- `devctl/tests/journey/m50_deploy.rs`: порядок по зависимостям, круг и неизвестная зависимость, план соцсети; подъём и остановка без сирот в обратном порядке; упрямый процесс убит после дедлайна; сорвавшийся подъём гасит поднятое; перезапуск по одному.
- `api-gateway/tests/journey/m50_deploy.rs`: шлюз со списком реплик, одна мертва: шесть постов подряд проходят.
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: `tools/devctl` и `tools/healthcheck` в воркспейсе; `docker-compose.yml` и семь Dockerfile; в Dockerfile `FROM .* AS `, `USER `, кэш, `HEALTHCHECK`, exec-форма и никакого `COPY .env`; compose ждёт `service_healthy`, секрет файлом; `*_FILE`; порядок, SIGKILL после дедлайна, откат сорвавшегося подъёма; круг по здоровым репликам; задачи `up` и `down`; и все договорённости прошлых вех.

## Как запустить

NATS курса должен работать: `127.0.0.1:54222`.

```
cd backend
cargo make check
cargo make up
cargo run -p devctl -- status
cargo run -p social-cli
```

В клиенте пиши посты и листай ленту, а во втором терминале `cargo run -p devctl -- restart post-service`: экземпляры перезапускаются по одному, шлюз в это время ходит в живой, и у клиента ни одной ошибки. `cargo make down` гасит всё в обратном порядке. На машине с Docker: `mkdir -p secrets && head -c 32 /dev/urandom | base64 > secrets/link_key`, `cargo xtask certs`, затем `docker compose up --build`.
