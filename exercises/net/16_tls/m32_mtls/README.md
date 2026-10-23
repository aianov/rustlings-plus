# Веха 32. Шифрование

Пока всё ходит открытым текстом. Токен входа летит в заголовке, который читает любой в той же сети кафе; а внутри кластера post-service поверит любому, кто дотянулся до его порта и положил `x-user-id`. Эта веха включает TLS везде: клиенты видят `https://` и `wss://`, сервисы друг с другом говорят по взаимному TLS и узнают собеседника по сертификату, а post-service пускает только шлюз. Свои движки при этом не трогаем: TLS снимается терминатором перед ними.

Собирается всё из главы 16: свой CA и сертификаты через rcgen, rustls и tokio-rustls, проверка имени сервера, ALPN, mTLS и личность из SAN, TLS в tonic.

## Что сделать

1. **Корень.** В `[workspace.dependencies]`: у `tonic` фича `tls-ring` (`tonic = { version = "=0.14.6", features = ["tls-ring"] }`), `rcgen = { version = "=0.13.2", default-features = false, features = ["ring", "pem"] }`, `x509-parser = "0.16"`, `rustls = { version = "0.23", default-features = false, features = ["ring", "std", "tls12"] }`, `rustls-pki-types = "1"`, `tokio-rustls = { version = "0.26", default-features = false, features = ["ring", "tls12"] }`, пути `certgen` и `tls-edge`.
   - `.gitignore` корня: `/target`, `.env`, `certs/`.
   - `.env.example` (и `.env`): `CERTS_DIR=certs`, `SOCIAL_CA=certs/ca.pem`.
   - `Makefile.toml`: задача `certs` (`cargo xtask certs --keep`), `run-all` и `demo` зависят от неё; в `Makefile` то же.

2. **src/crates/certgen** (только `rcgen`): `issue_all(dir, force) -> io::Result<Vec<PathBuf>>` выпускает `ca.pem`, `ca.key` (CA с `BasicConstraints::Constrained(0)`) и для каждого из `SERVICES = [api-gateway, auth-service, user-service, post-service, chat-service]` пару `<имя>.pem`, `<имя>.key`: SAN это имя сервиса, `localhost` и `127.0.0.1`, назначение ServerAuth и ClientAuth. Ключи пишутся с правами 0600. Если `ca.pem` уже есть, без `force` это ошибка `AlreadyExists` с подсказкой про `--force`: сервисы доверяют этому CA. `ensure_gitignore(root, entry) -> io::Result<bool>` дописывает строку один раз.

3. **tools/xtask**: `cargo xtask certs` (выпустить), `--force` (перевыпустить), `--keep` (оставить, если уже есть), незнакомый флаг это ошибка; после выпуска `certs/` в `.gitignore`. Справка называет `certs`.

4. **src/crates/tls-edge** (`rustls`, `tokio-rustls`, `tokio`, `service-runtime`): `server_config(cert_pem, key_pem)` и `client_config(ca_pem)` (провайдер ring, ALPN `http/1.1`), `terminate(public, inner, config, shutdown)`: принимает TCP на публичном порту, делает рукопожатие (не дольше `HANDSHAKE` 10 с), соединяется с движком на `inner` и гоняет байты в обе стороны; на остановке перестаёт принимать и даёт открытым соединениям доработать до `DRAIN` 5 с.

5. **grpc-kit**: `tls/`.
   - `Tls { name, ca, cert, key }` (Debug не печатает ключ), `Tls::load(dir, name)` (нет файла: ошибка с подсказкой `cargo xtask certs`), `Tls::from_env(name)` (`CERTS_DIR`, по умолчанию `certs`), `server()` (`ServerTlsConfig` с `identity` и `client_ca_root`: без клиентского сертификата рукопожатие не пройдёт), `client(peer)` (`ClientTlsConfig` с CA, своей `identity` и `domain_name(peer)`).
   - `channel(addr, peer, Option<&Tls>)`: `https://` с TLS или `http://` без него, всегда `connect_lazy`.
   - `peer_names(&request)`: DNS-имена из SAN сертификата собеседника (`peer_certs`, x509-parser).
   - `Gate`: `Gate::open()` и `Gate::only(&["api-gateway"])`; перехватчик сначала сверяет SAN клиента (не тот: `PERMISSION_DENIED` с именами), потом делает то же, что `caller`.

6. **Сервисы**: `Config.tls: Option<Tls>` (в тестах прошлых вех `None`, и всё ходит как раньше). С TLS: `Server::builder().tls_config(tls.server())`, клиенты к user-service через `channel(.., "user-service", tls)`. post-service с TLS ставит `Gate::only(&[GATEWAY])` (`GATEWAY = "api-gateway"`), без TLS `Gate::open()`. auth-service отдаёт HTTP входа через `terminate`. `Error` получает `Tls` и `Certs`. `main.rs` каждого сервиса читает `Tls::from_env(имя)`: в бою без сертификатов сервис не стартует, а ошибка говорит, что запустить.

7. **api-gateway**: `Config.tls`. С TLS публичный порт слушает `terminate`, движок `http-engine` переезжает на `127.0.0.1:0`, а каналы к сервисам идут по mTLS (`Upstreams::connect(&services, tls)`). `http-engine` и `websocket-engine` не меняются: в них нет ни строчки rustls.

8. **tools/social-cli** приходит новой версией: `net/` (`Dialer::plain()`, `Dialer::trusting(ca_pem)`, `Stream` поверх TCP или TLS), `Web::with`, `Ws::connect_with`, `connect_with`, `run_with(gateway, auth, dialer, ..)`, флаг `--ca` и `SOCIAL_CA`; без `--ca` на TLS-порт клиент не падает, а подсказывает запустить с `--ca certs/ca.pem`.

## Дерево после вехи

```
backend/
  .gitignore                          /target, .env, certs/
  certs/                              создаёт cargo xtask certs, в репозиторий не идёт
  src/crates/
    certgen/src/issue/ gitignore/     свой CA и сертификаты сервисов
    tls-edge/src/config/ terminate/   TLS-терминатор перед движками
    grpc-kit/src/tls/                 Tls, channel, peer_names, Gate
  src/microservices/*/                + Config.tls, tls_config, main.rs читает сертификаты
  tools/xtask/src/certs.rs            cargo xtask certs
  tools/social-cli/                   клиент курса с https и wss (новая версия)
```

## Приёмка

- `api-gateway/tests/journey/m32_mtls.rs`: весь стек по TLS (сертификаты выпускает `certgen` во временную папку): регистрация по HTTPS, сокеты по WSS, пуши, профиль и `/health`; открытый текст на порт шлюза не получает ответа, клиент с чужим CA не верит шлюзу, каждый сервис не говорит открытым текстом и не пускает сертификат чужого CA даже с именем `api-gateway`; post-service отказывает user-service, chat-service и auth-service с `PERMISSION_DENIED` и пускает шлюз; клиент курса ходит по `https://` и `wss://`;
- `certgen`, `tls-edge`, `grpc-kit`, `xtask`: свои `m32_mtls.rs` (SAN, подпись CA, права ключей, отказ без `--force`, строка в `.gitignore`; терминатор пропускает TLS к движку и не пропускает открытый текст и чужой CA; взаимный TLS требует клиентский сертификат, сверяет имя сервера, `Gate` по SAN, подсказка про `cargo xtask certs`; команда `cargo xtask certs` с флагами);
- новая версия `support.rs`: `Gateway::secure(имя, config)`, `dialer`, `certs`, `web()`, `auth_web()`, `Peer::connect_with`, вход и регистрация через `Web`;
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву, `certs/` в `.gitignore`, ни `rustls` в `http-engine` и `websocket-engine`, ни «принимаю всё» нигде, `client_ca_root` и `identity` клиента в grpc-kit, `peer_certs` и `Gate::only(&[GATEWAY])` в post-service, `tls_config` у каждого gRPC-сервера, `terminate` у шлюза и auth-service, `Tls::from_env` в каждом `main.rs`, задача `certs`, зависимости по слоям, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo xtask certs
cargo make check
cargo make demo
```

`demo` сам выпустит сертификаты, если их ещё нет, и запустит клиента с `SOCIAL_CA` из `.env`: в первой строке он скажет `шлюз https://...`. Второе окно: `cargo run -p social-cli -- --ca certs/ca.pem`. Без `--ca` клиент подскажет, что сервер ждёт TLS. Посмотреть, что внутри: `openssl x509 -in certs/post-service.pem -noout -text | grep -A1 Alternative`.
