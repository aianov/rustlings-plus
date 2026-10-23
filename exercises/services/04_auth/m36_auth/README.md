# Веха 36. Вход по-настоящему

До сих пор вход в соцсеть был вежливой формальностью: ник без пароля, токен это 32 случайных символа в памяти auth-service, живёт вечно и проверяется поиском по таблице. Выйти можно только с одного устройства, роль у всех одна, а чужой пост может удалить разве что сам автор. Эта веха делает вход настоящим: пароли на argon2, подписанный access-токен с коротким сроком, refresh в HttpOnly cookie с ротацией, выход везде, смена и сброс пароля по одноразовой ссылке, роли с запретом по умолчанию и права на посты.

Собирается всё из главы 04_auth: упражнения с svauth1 по svauth21, проект svauth_service и nha2. Подпись, base64url и порядок проверки токена пишутся руками: крейты `jsonwebtoken`, `hmac` и `base64` в auth-service запрещены.

## Что сделать

1. **Корень**: в `[workspace.dependencies]` `argon2 = "0.5"` и `sha2 = "0.10"`; `[profile.dev.package.argon2]` и `[profile.dev.package.blake2]` с `opt-level = 3`, иначе argon2 в отладочной сборке считается секундами. В `.env.example` и `.env`: `AUTH_ACCESS_TTL_SECS`, `AUTH_REFRESH_TTL_SECS`, `AUTH_MODERATORS` и закомментированный `AUTH_SIGNING_KEYS`.

2. **contracts**: `Role { Guest, User, Moderator, Admin }` (`parse` строгий: незнакомая строка это `Guest`, `as_str`, `allows`), `Permission { Write, EditOwnPost, DeleteOwnPost, DeleteAnyPost }` и таблица `GRANTS` с запретом по умолчанию. Код `expired` (401) в каталоге. `Login { token, user, expires_in }`, `Credentials { username, password }` с `Debug` без пароля. Вызов `posts.edit` (`Call::EditPost { user, post, text }`).

3. **proto**: `Account.role`; `rpc EditPost(EditPostRequest { post_id, text }) returns (Post)`; `ValidateToken` отвечает `UNAUTHENTICATED` с кодом `expired` или `unauthorized`. В `status`: `Expired` это `Unauthenticated`.

4. **grpc-kit**: `Stamp::role(..)` кладёт роль в `x-user-role`, перехватчик `caller` переносит её в `CallerRole`, `role_of(&request)` (без роли это `guest`), `Stamp::follow` передаёт роль дальше.

5. **service-runtime**: `Fields::secret_optional`; `Fields` переехал в `config/fields.rs`.

6. **auth-service** (зависимости `argon2`, `sha2`, `serde_json`).
   - `service/token/`: `base64.rs` (base64url без паддинга; строгий декодер отвергает `=`, пробел, лишние ненулевые биты и длину с остатком 1), `hmac.rs` (`hmac_sha256` по RFC 2104 поверх sha2, ключ длиннее блока сначала хешируется; `same` за постоянное время; `sha256_hex`, `hex`), `keys.rs` (`KeyRing`: текущий ключ подписывает, прошлые проверяют, `rotate`, `retire`, `parse("kid:ключ_base64url,...")`, `random()`, ключ не короче 32 байт, `Debug` без ключей), `mod.rs` (`sign`, `verify` в порядке: части, `alg` строго `HS256`, `kid` из связки, подпись, и только потом `exp`, `nbf`, `iss`, `aud` с допуском; `VerifyError`, `TokenPolicy`). Формат `Header` и `Claims { sub, name, role, iss, aud, iat, nbf, exp, jti }` лежит в `common/claims.rs`.
   - `service/password/`: `Cost { memory_kib, iterations, parallelism }` (`STRONG` для боя, `QUICK` для тестов), `Passwords::new(cost, parallel)` с фальшивым хешем для неизвестных ников, `hash` и `verify` в `spawn_blocking` под `Semaphore`, `Verdict { Wrong, Right { rehashed } }`: при верном пароле и выросших параметрах запись пересчитывается.
   - `repository/`: `Account { user_id, username, email, email_verified, password, role }` (старый журнал читается, `Debug` без хеша), `AccountRepository::update`; `MemorySessions` (`SessionRow { family, user_id, used, expires, access_jti, access_exp }`; `open`, `rotate` одной операцией под замком: неизвестный, погашенный, повторный (гасит семейство и запрещает его access) или просроченный (`now >= expires`); `revoke_family`, `revoke_user(user, keep)`, `deny`, `is_denied`, `purge`); `MemoryOneTime` (`Purpose { ConfirmEmail, ResetPassword }`, новая ссылка отменяет прежнюю, `take` атомарно и только своего назначения). `tokens.rs` удалён.
   - `service/`: `AuthService::new(AuthParts { accounts, sessions, once, passwords, keys, clock, mailer, lifetimes, moderators, public_url })`, `AuthParts::memory(clock, mailer)` для тестов, `Lifetimes { access, refresh, confirm, reset, leeway }`; `register(&dyn ProfileDirectory, &Registration)` (роль `moderator` для ников из `AUTH_MODERATORS`), `login` (одинаковый ответ на чужой ник и неверный пароль, перехеширование), `refresh` (ротация), `validate -> Principal { user_id, username, role, jti, expires }` (истёкший это `expired`, отозванный `jti` это `unauthorized`), `logout`, `logout_all`, `change_password` (гасит всех, кроме текущей сессии), `request_confirmation`, `confirm_email`, `forgot_password` (на чужой ник молча), `reset_password` (гасит все сессии), `purge`, `rotate_key`, `retire_key`. Порт `Mailer` и `Letter { to, subject, link }`; `client/mail.rs`: `LogMailer` (письмо в журнал процесса) и `Mailbox` (письма в памяти). Секреты только из `getrandom`.
   - `controller/`: HTTP `POST /auth/register {username, password, email?}`, `/auth/login {username, password}` (тело `Login`, cookie `session` с access и `refresh` с `HttpOnly`, `Path=/auth`, `SameSite=Strict`), `/auth/refresh` (из cookie; при отказе cookie стираются), `/auth/logout`, `/auth/logout-all`, `/auth/password {old_password, new_password}`, `/auth/email/request`, `GET` и `POST /auth/email/confirm`, `/auth/password/forgot {username}` (всегда 202), `/auth/password/reset {token, password}`; защищённые маршруты отвечают 401 с кодом `expired` или `unauthorized`. gRPC `ValidateToken` отдаёт `role`.
   - `dto/`: пароль 8..=128 символов и не равен нику, `PasswordChangeBody`, `ForgotBody`, `ResetBody`, `ConfirmBody`.
   - `common/config.rs`: `AUTH_SIGNING_KEYS` (секрет; без него ключ создаётся при старте и журнал об этом пишет), `AUTH_ACCESS_TTL_SECS` (900), `AUTH_REFRESH_TTL_SECS` (30 дней), `AUTH_CLOCK_LEEWAY_SECS` (30), `AUTH_ARGON_MEMORY_KIB`, `AUTH_ARGON_ITERATIONS`, `AUTH_HASH_PARALLEL`, `AUTH_MODERATORS`, `AUTH_PUBLIC_URL`; `Config::default()` берёт `Cost::QUICK`.
   - Фоновая задача «чистка сессий» (раз в минуту `purge`) под надзором `Background`.

7. **post-service**: `Actor { id, role }` (из `UserId` получается `User`), `PostService::edit(actor, post, PostText)` правит только автор, `delete(actor, post)`: автор с `DeleteOwnPost` или роль с `DeleteAnyPost`; проверки в service, контроллер только собирает `Actor` из `caller_of` и `role_of`. Журнал `Edited { post, text }`, `PostRepository::edit`.

8. **api-gateway**: `RemoteAuth::check -> Option<(UserView, Role)>`, истёкший токен это 401 `expired`; соединение помнит роль (`Ctx::acting_as`), каждый вызов несёт её в штампе; WS-метод `posts.edit`.

9. **tools/social-cli** приходит новой версией: `register <ник> <пароль> [почта]`, `login <ник> <пароль>`, `logout all`, `password <старый> <новый>`, `confirm`, `forgot <ник>`; клиент хранит cookie `refresh`, на 401 сам меняет её на новый access и переоткрывает WebSocket («токен обновлён по refresh»), а если refresh не принят, говорит «сессия закрыта: войди заново».

## Дерево после вехи

```
backend/src/
  crates/contracts/src/role/                  Role, Permission, запрет по умолчанию
  crates/service-runtime/src/config/fields.rs Fields
  microservices/auth-service/src/
    service/token/ base64.rs hmac.rs keys.rs  подпись, base64url, связка ключей
    service/password/                         argon2 на отдельном пуле
    service/exits.rs letters.rs mail.rs       выход, письма, порт почты
    repository/sessions.rs once.rs            семейства refresh, одноразовые токены
    client/mail.rs                            LogMailer, Mailbox
    common/claims.rs                          Header, Claims
tools/social-cli/src/client/session.rs        refresh и повторный вход (новая версия)
```

## Приёмка

- `auth-service/tests/journey/m36_auth.rs`: base64url по RFC 4648 и строгий разбор, HMAC по векторам RFC 4231 (с ключом длиннее блока), `none` и чужой `kid` отвергаются, подделка с испорченной подписью отвергается раньше, чем читается срок, граница срока ровно `exp + leeway`, `nbf`, чужие `aud` и `iss`, смена ключа никого не разлогинивает до `retire`, argon2id с солью и перехешированием, хеш не замораживает однопоточный рантайм.
- `m36_sessions.rs`: неверный пароль и чужой ник неразличимы, ротация refresh и гашение всего семейства при повторе (другое устройство живо), строка хеша из хранилища не открывает вход, отдельный код `expired` и точная граница refresh, выход, выход везде, смена пароля, одноразовые ссылки (новая отменяет старую, назначение, однократность, час на сброс), роль moderator из конфига в токене; `m36_http.rs`: cookie `refresh` с `HttpOnly`, `Path=/auth`, `SameSite=Strict`, ротация по HTTP, повтор стирает cookie, слабый пароль это 422, выход везде через HTTP.
- `post-service`: таблица ролей с запретом по умолчанию, автор правит, модератор удаляет чужое, пользователь и гость нет.
- `api-gateway`: правка и удаление по WebSocket с ролью из токена, истёкший токен это 401 `expired` и для HTTP, и для сокета, клиент курса сам обновляет токен, а «выйти везде» закрывает второе окно.
- новые версии тестов прошлых вех: регистрация и вход везде с паролем (`support.rs` шлюза, сценарии клиента в m24, m26, m27, m29, m31, m32, m35, тела в m25 и m30 шлюза, m30 и m35 auth-service, `http_api.rs`), формат токена в m25 шлюза, `fakes.rs` (`Account.role`), m13, m25 и m35 contracts (`posts.edit`, `expires_in`, `password`, код `expired`), m30 proto, m33 auth-service (`AuthParts::memory`), m30 post-service (роль `user` в штампе);
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: файлы по дереву, `jsonwebtoken`, `hmac` и `base64` запрещены в auth-service, `tokens.rs` отсутствует, `UnsupportedAlg`, `hmac::same` и проверка аудитории в `token/mod.rs`, `0x36` в `hmac.rs`, `rotate` в связке ключей, `Argon2id`, `spawn_blocking` и `Semaphore` в хешере, `RefreshError::Reused` в сессиях, хеш refresh в хранилище, нигде нет `seed_from_u64` и `thread_rng`, `http_only: true` у cookie, незнакомая роль это гость, `DeleteAnyPost` в service post-service и ни одного `Permission::` в контроллерах, `x-user-role` в grpc-kit, и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

В клиенте: `register alice correct-horse alice@example.com`, `login alice correct-horse`, `confirm` (ссылка появится в журнале auth-service), `post привет`. Во втором окне `cargo run -p social-cli -- --ca certs/ca.pem` и `login alice correct-horse`. В первом `logout all`: следующая команда во втором окне ответит «сессия закрыта: войди заново». Чтобы увидеть автообновление, запусти стек с `AUTH_ACCESS_TTL_SECS=5 AUTH_CLOCK_LEEWAY_SECS=0 cargo make demo`: через пять секунд `me` сначала напишет «токен обновлён по refresh». Модератор по умолчанию это ник `moder` (`AUTH_MODERATORS` в `.env`): он удаляет чужие посты командой `delete <id>`.
