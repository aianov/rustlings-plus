# Веха 49. Аудит безопасности

Соцсеть переживает нагрузку и соседей, но не злоумышленника. Вход можно перебирать сколько угодно быстро. Удалить чужой комментарий нельзя только потому, что такой кнопки нет. Ссылка в посте могла бы превращаться в превью, но наивный запрос по ссылке это дверь во внутреннюю сеть: `http://169.254.169.254/` или `http://127.0.0.1:9880/metrics`. refresh по cookie обменивается на токен любой страницей, которую открыл пользователь. Ник с переводом строки в заголовке ответа разрезал бы ответ надвое. А шаблон валидатора на хитрой строке перебирает варианты так долго, что один запрос занимает воркер на минуту.

Эта веха проходит по соцсети с аудитом. Вход отвечает одинаково и за одинаковое время, а за промахи ждёт растущую задержку по аккаунту и по адресу, никого не блокируя. Удалять комментарий может автор или автор поста, черновики видит только владелец, а чужой черновик выглядит несуществующим. Превью ссылок ходит наружу через http-client со сторожем адресов: только http(s), проверка адреса после резолва, соединение на проверенный адрес, заново на каждом редиректе, таймаут и лимит размера. refresh по cookie требует свой заголовок. Заголовки не режутся переводом строки, большие тела отвергаются до разбора, коллекции ограничены, а шаблон валидатора идёт автоматом без перебора.

Всё собирается из главы 18_security: задачи с svsec1 по svsec14 и аудит svsec_audit, плюс http-клиент из nhc1–nhc11 и ntcp11.

## Что сделать

1. **http-client**: `guard/`: `AddrGuard::new(проверка)`, `AddrGuard::public_only()`, `public_ip(ip)`: закрыты loopback, частные, link-local, unspecified, broadcast, multicast, документационные, 0/8, 100.64/10, 198.18/15, 240/4, а для IPv6 loopback, unspecified, multicast, fc00::/7, fe80::/10, 2001:db8::/32 и IPv4 внутри `::ffff:`; `Config.guard`: с ним клиент резолвит имя, требует, чтобы все адреса прошли проверку (`Error::Forbidden`), и соединяется на проверенный адрес; на каждом редиректе проверка повторяется.

2. **http-engine**: `header_safe` (CR, LF и NUL в значении становятся пробелами) и `token_name` (имя заголовка из символов token); `Headers::append` и `set` применяют их.

3. **validation**: `Pattern::is_match` идёт по тексту один раз, держа множество состояний (элемент шаблона, сколько повторов); у неограниченного повтора счётчик упирается в минимум, поэтому состояний не больше, чем элементов шаблона на их пределы.

4. **post-service**:
   - клиенты стали папками: `client/relay/`, `client/users/`, `client/preview/`;
   - `Previewer` (`new` с `public_only`, `with_guard`, `title(url)`): только http и https, таймаут `PREVIEW_TIMEOUT` (2 с), `PREVIEW_MAX_BODY` (64 КиБ), `PREVIEW_REDIRECTS` (3), только `text/html`; `extract_title`: `<title>` в любом регистре, сущности, пробелы, не длиннее `TITLE_MAX` = 200 символов; `PostsController::with_previews` заполняет заголовки ссылок без заголовка при создании поста; включается `POSTS_PREVIEW`;
   - `DeleteComment` (автор комментария, автор поста или модератор, иначе `forbidden`), `SaveDraft`, `GetDraft`, `DeleteDraft` (только владелец, чужой черновик это `not_found`); `repository/drafts.rs` (`DraftRepository`), `controller/drafts.rs`, записи журнала `CommentRemoved`, `DraftSaved`, `DraftDeleted`; `PostService::with_drafts`.

5. **auth-service**:
   - `service/throttle.rs`: `Throttle` (`THROTTLE_BASE` 100 мс вдвое за промах до `THROTTLE_CAP` 5 с, промахи забываются через 15 минут), ключи `ThrottleKey { Account, Address }`; `AuthService::login_from(ник, пароль, адрес)` ждёт задержку по обоим ключам, неудача добавляет промах обоим, успех снимает промахи аккаунта (адреса нет: иначе перебор чередовался бы со своим аккаунтом); адрес из `X-Real-IP` шлюза или собеседника;
   - фиктивный хеш для несуществующего аккаунта остаётся: ответ и время у неизвестного ника и неверного пароля одинаковые;
   - `POST /auth/refresh` без заголовка `X-Social-Client` это 403: чужая страница пришлёт cookie, но свой заголовок не сможет.

6. **Шлюз**: `POST /drafts`, `GET /drafts/:id`, `DELETE /drafts/:id`, `DELETE /comments/:id` (Bearer обязателен: `OWNED` в охране); CORS разрешает `X-Social-Client`; Origin у WebSocket по тому же белому списку (без Origin это не браузер, пускаем).

7. **Окружение**: `POSTS_PREVIEW=true` в `.env.example` и `.env`.

Клиент курса приходит готовым: обмен refresh шлёт `X-Social-Client`.

## Дерево после вехи

```
backend/src/
  crates/http-client/src/guard/mod.rs
  microservices/post-service/src/
    client/
      mod.rs
      preview/mod.rs
      relay/mod.rs
      users/mod.rs
    controller/drafts.rs
    repository/drafts.rs
  microservices/auth-service/src/service/throttle.rs
  microservices/api-gateway/src/controller/drafts.rs
```

Остальное дерево прежнее.

## Приёмка

Тестам нужен NATS курса с JetStream на `127.0.0.1:54222` (адрес из `RUSTLINGS_NATS_URL`).

- `http-client/tests/journey/m49_security.rs`: таблица публичных и внутренних адресов; loopback и `localhost` отвергнуты до соединения; редирект проверяется заново.
- `http-engine/tests/journey/m49_security.rs`: перевод строки не режет ответ, плохое имя заголовка не пишется.
- `validation/tests/journey/m49_security.rs`: враждебный шаблон на 5000 символов за доли секунды.
- `post-service/tests/journey/m49_security.rs`: чистка заголовка; превью публичной страницы, не HTML и чужие схемы без превью; внутренние адреса, редирект внутрь и большая страница без превью; владельцы комментариев и черновиков.
- `auth-service/tests/journey/m49_security.rs`: задержка растёт по ключу, не трогает чужой аккаунт, снимается входом и забывается; неизвестный ник и неверный пароль неотличимы, четвёртая попытка ждёт, хозяин входит; refresh без своего заголовка это 403.
- `api-gateway/tests/journey/m49_security.rs`: черновики и комментарии по HTTP; WebSocket и CORS для чужого сайта; ник с переводом строки, тело больше лимита по заголовку, слишком много тегов.
- новые версии тестов прошлых вех: m36 auth-service (refresh со своим заголовком), `fakes.rs` (четыре новых RPC post-service).
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: превью в `client/preview/mod.rs` с `public_only` и лимитами; соединение на проверенный адрес; список внутренних сетей; задержка по адресу и аккаунту без блокировки; фиктивный хеш; заголовок refresh; владельцы комментария и черновика; `header_safe`; нет `matches_from` в шаблоне; `throttle.rs` в разрешённых детях service auth-service; и все договорённости прошлых вех.

## Как запустить

NATS курса должен работать: `127.0.0.1:54222`.

```
cd backend
cargo make check
cargo make demo
```

В клиенте: `register alice correct-horse`, `login alice correct-horse`, `post почитай http://example.com`: в ленте у ссылки появится заголовок «Example Domain». `post а тут http://127.0.0.1:9880/metrics` создаст пост без заголовка у ссылки: внутренний адрес превью не получает. Пять раз `login alice неверный` подряд: каждая следующая попытка заметно дольше, а `login alice correct-horse` всё равно пускает.
