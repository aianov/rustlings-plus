# Веха 43. Аватары как файлы

С m27 аватар живёт в памяти: шлюз принимает тело целиком, user-service держит байты в `HashMap`, а раздача идёт по `/avatars/<id>` любому, кто знает номер. После перезапуска аватары пропадают. Тип файла берётся из `Content-Type`, так что под видом картинки можно загрузить что угодно. Ссылка на аватар вечная, а пять мегабайт одним gRPC-сообщением упираются в лимит и лежат в памяти обоих сервисов.

Эта веха делает аватары файлами. user-service пишет их на диск через порт `FileStore`: временный файл, `sync_all`, `rename`, а брошенный временный файл убирает страж в `Drop`. Ключ файла это sha256 содержимого, поэтому одинаковые картинки занимают одно место. Шлюз шлёт аватар в user-service потоком кусков (gRPC client-streaming), лимит считается на лету. Тип определяется по сигнатуре (PNG, JPEG, WebP), размеры читаются из заголовка картинки и ограничены. Профиль отдаёт подписанную ссылку со сроком: HMAC от ключа, срока и имени файла. Шлюз раздаёт файл по ней с Range, `nosniff` и `Content-Disposition`. Загрузка двухфазная: сначала запись о незавершённой загрузке, потом ссылка из профиля, а сирот через час убирает фоновая задача одного экземпляра.

Всё собирается из главы 11_files: задачи с svfile1 по svfile13 и проект svfile_quiz, плюс client-streaming из ntx_client_stream, Range из nha7 и nha8 и HMAC из svauth6.

## Что сделать

1. **Крейт `src/crates/files`** (зависимости только tokio, futures, sha2):
   - `store/`: `Chunks` (поток `io::Result<Vec<u8>>`), `chunks_of(data, size)`, `valid_key(key)` (от 3 до 128 символов `a-z0-9-_.`, не с точки), трейт `FileStore { put, get, exists, rename, remove }` и `FsFileStore::new(root)`: файл лежит в `root/<первые два символа ключа>/<ключ>`, запись идёт во временный `.<ключ>.<номер>.tmp`, потом `flush`, `sync_all` и `rename`; страж `TempGuard` в `Drop` удаляет временный файл, если до `rename` не дошли. Чужой ключ (`../x`) это ошибка;
   - `limit/`: `limit(body, max, declared)`: заявленный размер больше `max` это `FileError::TooLarge` сразу, иначе поток считает байты и на куске, перешедшем предел, отдаёт ошибку и заканчивается;
   - `content/`: `put_content(store, body, temp) -> Stored { key, size, existed }`: хеш sha256 считается тем же проходом, что и запись во временный ключ; если файл с таким хешем уже есть, временный удаляется (`existed: true`), иначе переименовывается;
   - `image/`: `Kind { Png, Jpeg, Webp }` (`mime`, `ext`, `parse`), `sniff(head)`, `check_avatar(head) -> Picture { kind, width, height }`: PNG читает IHDR, JPEG идёт по сегментам до SOF (кроме C4, C8, CC), WebP понимает VP8X, `VP8 ` и VP8L; стороны от 1 до `MAX_SIDE` = 4096, площадь (в u64) не больше `MAX_PIXELS` = 16 000 000; `ImageError { NotAnImage, Truncated, BadSize }`; `HEAD_MAX` = 64 КиБ;
   - `sign/`: свой `hmac_sha256(key, msg)`, `Signer::new(key)`, `link(key, exp, name)` даёт `/files/<key>?exp=<exp>&name=<name>&sig=<hex>` (подписано `key\nexp\nname`), `verify(path_and_query, now) -> Linked { key, name, exp }`: сначала формат (лишний или повторный параметр, плохой ключ или имя это `Format`), потом подпись за постоянное время (`Signature`), потом срок (`now >= exp` это `Expired`);
   - `serve/`: `safe_headers(head, name) -> (Disposition, заголовки)`: `Content-Type` по сигнатуре, незнакомое это `application/octet-stream` и `attachment`, `Content-Disposition` с очищенным именем и `filename*=UTF-8''...`, `X-Content-Type-Options: nosniff`, `Content-Security-Policy: default-src 'none'; sandbox`.

2. **proto**: `rpc UploadAvatar(stream AvatarChunk) returns (PutAvatarReply)` вместо `PutAvatar`, `rpc GetFile(FileRequest) returns (Avatar)`; `PutAvatarReply { created, key, content_type }`, у `Avatar` поле `key`, у `Profile` поля `avatar` (ключ) и `avatar_type`.

3. **user-service**:
   - `Avatar { key, content_type }` вместо байтов в памяти; `AvatarRepository` хранит ссылки владелец → файл и незавершённые загрузки: `link`, `get`, `remove`, `referenced(key)`, `pending(key, at)`, `settle(key)`, `stale(before)`;
   - `service/avatars.rs`: `put_avatar(owner, body, declared)`: профиль есть, `limit` с `AVATAR_MAX`, голова копится, пока `check_avatar` не скажет да или нет (не больше `HEAD_MAX`), временный ключ `tmp-<owner>-<микросекунды>` записан как незавершённый, `put_content`, итоговый ключ записан как незавершённый, ссылка в профиле, записи сняты; прежний файл удаляется, если на него больше никто не ссылается. `delete_avatar` тоже убирает файл без ссылок. `sweep(now, grace)` удаляет незавершённые старше `grace` (`ORPHAN_GRACE` = час), если на них нет ссылки;
   - слой `service` диска не касается: ни `std::fs`, ни `tokio::fs`, только `FileStore`;
   - `UploadAvatar` принимает `Streaming<AvatarChunk>`, `GetFile` отдаёт файл по ключу, профиль несёт ключ и тип аватара;
   - конфиг `USERS_FILES_DIR` (по умолчанию `<temp>/social-files`) и `USERS_ORPHAN_SWEEP_MS` (10 минут); с NATS уборку сирот ведёт `Leader` «orphans» с арендой из m42.

4. **Шлюз**:
   - `PUT /users/me/avatar`: заявленный и пришедший размер сверяются с `avatar_max` до похода в user-service (413), тело уходит кусками по 16 КиБ, ответ 201 или 204 с `ETag` и `Location` (подписанная ссылка); тип больше не берётся из `Content-Type`, не картинка это 415 от user-service; у загрузки свой бюджет `GATEWAY_UPLOAD_BUDGET_MS` (10 с);
   - `GET /files/:key?exp&name&sig`: проверка ссылки (400 испорчена, 403 чужая подпись, 410 срок вышел), `GetFile`, раздача с Range, ETag и заголовками `safe_headers`;
   - `/avatars/:id` больше нет; `UserView` несёт `avatar` (подписанная ссылка со сроком `GATEWAY_LINK_TTL_MS`, час) во всех ответах профиля, HTTP и WebSocket;
   - `common/links.rs`: `Links { signer, ttl }` с `avatar(key, content_type)` и `verify(target)`; ключ подписи `GATEWAY_LINK_KEY` от 16 байт.

5. **contracts**: у `UserView` поле `avatar: Option<String>` (без аватара его нет в JSON).

6. **Окружение**: `GATEWAY_LINK_KEY`, `GATEWAY_LINK_TTL_MS`, `GATEWAY_UPLOAD_BUDGET_MS`, `USERS_FILES_DIR`, `USERS_ORPHAN_SWEEP_MS` в `.env.example` и `.env`; в профиле dev `sha2` собирается с `opt-level = 3`, иначе хеш пяти мегабайт не влезает в бюджет.

Клиент курса приходит готовым: `avatar set` печатает ссылку, `avatar get <ник> [файл]` берёт ссылку из профиля и докачивает по Range, на просроченную отвечает «ссылка на аватар просрочена».

## Дерево после вехи

```
backend/src/
  crates/files/
    Cargo.toml
    src/
      lib.rs
      store/mod.rs        FileStore, FsFileStore, Chunks, valid_key
      limit/mod.rs        limit, FileError
      content/mod.rs      put_content, Stored, hex
      image/mod.rs        Kind, sniff, check_avatar, Picture
      sign/mod.rs         Signer, hmac_sha256, Linked, LinkError
      serve/mod.rs        safe_headers, Disposition
  microservices/user-service/src/service/avatars.rs
  microservices/api-gateway/src/common/links.rs
```

Остальное дерево прежнее.

## Приёмка

Тестам нужен NATS курса с JetStream на `127.0.0.1:54222` (адрес из `RUSTLINGS_NATS_URL`).

- `files/tests/journey/m43_avatars.rs`: ключи; файл появляется целым или никак, временных не остаётся; ключ это sha256, повтор не пишет второй файл; лимит до чтения и на лету; тип по сигнатуре и размеры из заголовка с пределами; HMAC по RFC 4231; ссылка со сроком, подделка подписи, срока, ключа и формата; заголовки раздачи.
- `user-service/tests/journey/m43_avatars.rs`: аватар хранится по содержимому, старый файл без ссылок удаляется; слишком большой, не картинка, огромные размеры и оборванная загрузка не оставляют файлов; сироты уходят после срока, файл со ссылкой остаётся.
- `api-gateway/tests/journey/m43_avatars.rs`: загрузка через шлюз, ссылка в профиле, Range, `nosniff` и имя файла, подделка подписи и имени это 403; просроченная ссылка это 410, профиль выдаёт новую; лимит шлюза до user-service.
- новые версии тестов прошлых вех: m27 и m30 шлюза (настоящая картинка, ссылка из ответа или профиля, поток вместо одного сообщения), m30 user-service (поток кусков), m31 proto (`AvatarChunk`), m13 и m25 contracts (`avatar` у `UserView`), `fakes.rs` (`UploadAvatar` и `GetFile`).
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву; `files` в воркспейсе и зависит только от разрешённого; в `service` сервисов нет `std::fs` и `tokio::fs`; `FileStore`, `sync_all`, `rename`, страж в `Drop`, sha256, лимит на лету, `sniff`, площадь в u64, сравнение подписи за постоянное время, `nosniff`; `UploadAvatar` потоком, двухфазная загрузка, лидер `orphans`; маршрут `/files/:key` вместо `/avatars/`; и все договорённости прошлых вех.

## Как запустить

NATS курса должен работать: `127.0.0.1:54222`.

```
cd backend
cargo make check
GATEWAY_LINK_TTL_MS=20000 cargo make demo
```

В клиенте: `register alice correct-horse`, `login alice correct-horse`, `avatar set photo.png` (любая PNG, JPEG или WebP). Клиент ответит «аватар загружен» и покажет ссылку `/files/...`. `find alice` показывает ту же ссылку в профиле, `avatar get alice` скачивает файл. Через двадцать секунд старая ссылка отвечает 410, а `avatar get alice` берёт из профиля свежую и снова скачивает. Текстовый файл с расширением `.png` не загрузится: тип решает сигнатура.
