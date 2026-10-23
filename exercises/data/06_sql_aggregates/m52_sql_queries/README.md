# Веха 52. Запросы соцсети в SQL

Данные уже в Postgres, но читаются по-старому: карточка профиля тянет полные списки подписчиков и подписок только ради двух чисел, «друзей» нет вовсе, лента не знает, лайкнул ли ты пост, а админ не видит, сколько постов и лайков было за неделю. Всё это одна строчка SQL на сторону базы и десятки строк перебора в памяти на стороне сервиса.

Эта веха пишет запросы соцсети в SQL: подписчики и подписки с профилями, взаимные подписки как «друзья» (таблица `follows` соединяется сама с собой), посты по списку id для гидрации ленты с отметкой «ты лайкнул» (LEFT JOIN) и без скрытых авторов (анти-join), счётчики профиля одним запросом без размножения строк и статистика по дням с пустыми днями (`generate_series`). Списки листаются курсором по id, а не OFFSET. У сервисов без базы те же запросы есть в памяти.

Всё собирается из глав данных: 05_sql_joins, 06_sql_aggregates (задачи pgjoin и pgagg, особенно pgagg_stats).

## Что сделать

1. **proto**: в `users.proto` rpc `Followers`, `Following`, `Friends` (`RelatedRequest { user_id, after, limit }` → `ProfileList`) и `Counters` (→ `UserCounters { followers, following }`); в `posts.proto` поле `Post.liked = 8`, rpc `GetPosts(PostsByIds { ids, hidden_authors })` → `FeedReply`, `AuthorCounters(AuthorPostsRequest)` → `AuthorTotals { posts, likes }`, `DailyStats(DailyRequest { days })` → `DailyReply { days: [Day { day, posts, likes }] }`.

2. **user-service**: трейт `SocialQueries` (followers, following, friends с курсором `after` и `limit`, counters); `repository/queries.rs` с SQL-константами для `PgUsers` (друзья это `follows mine JOIN follows theirs`, счётчики скалярными подзапросами) и тем же в памяти для `MemoryUsers`; `service/social.rs` (`Related`, лимит списка, `UserError::NoQueries`), `controller/social.rs`; bootstrap подключает `with_queries`.

3. **post-service**: трейт `PostQueries` (`by_ids`, `author_totals`, `liked`, `daily`); `repository/queries.rs` для `PgPosts` (`LEFT JOIN likes ... AND l.user_id = $2`, `NOT EXISTS (SELECT 1 FROM unnest($3) ...)`, `p.id = ANY($1)` с порядком как в запросе, `generate_series` по UTC-дням) и для `MemoryPosts`; `service/queries.rs` (не больше `MAX_IDS` id за раз, дни 1..=`MAX_DAYS`, статистику смотрит только роль с правом `Permission::ReadStats`); `controller/queries.rs`, а поток Watch переехал в `controller/stream.rs`; `GetPost`, `AuthorPosts` и `Feed` отмечают `liked` для вызывающего.

4. **contracts**: `PostView.liked` (в JSON только когда true), `StatsView.posts` и `StatsView.likes`, `PeopleView { users, next }`, `DailyView { days: [DayView] }`, право `Permission::ReadStats` у админа.

5. **Шлюз**: методы `users.followers`, `users.following`, `users.friends` (`{ target, after?, limit? }` → `PeopleView`) и `posts.daily` для вошедших (`{ days? }` → `DailyView`, отказ не админу приходит из post-service); карточка `users.profile` берёт `Counters` и `AuthorCounters` вместо полных списков; лента гидрируется `GetPosts`: приходит отметка «ты лайкнул», а удалённые посты выпадают; без ответа post-service лента отдаётся как есть.

6. **social-cli**: `followers <ник>`, `following <ник>`, `friends <ник>`, `stats [дней]`; у поста «ты лайкнул»; в карточке подписчики, подписки, посты и полученные лайки.

## Дерево после вехи

```
src/microservices/
├── user-service/src/
│   ├── repository/queries.rs
│   ├── service/social.rs
│   └── controller/social.rs
├── post-service/src/
│   ├── repository/queries.rs
│   ├── service/queries.rs
│   └── controller/{queries.rs, stream.rs}
└── api-gateway/src/routes/social.rs
```

## Приёмка

Тестам нужны Postgres курса на `127.0.0.1:55432` и NATS курса на `127.0.0.1:54222`.

- `user-service/tests/journey/m52_sql_queries.rs`: подписчики и подписки с именами, друзья только при взаимной подписке, курсор `after`, счётчики.
- `post-service/tests/journey/m52_sql_queries.rs`: порядок постов как в запросе, «ты лайкнул», теги, скрытый автор отсечён, счётчики автора, лайкнутые из списка, статистика за три дня с пустыми днями.
- тесты прошлых вех проходят (карточка теперь со счётчиками постов и лайков), `cargo make check` зелёный.

Архитектура: файлы из дерева на месте; self join для друзей; курсор вместо OFFSET; счётчики скалярными подзапросами; LEFT JOIN для «ты лайкнул»; анти-join через NOT EXISTS; `generate_series`; `ANY($1)`; право ReadStats в service; гидрация ленты в шлюзе; и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

Зарегистрируй alice, bob и carol. alice и bob подписываются друг на друга, carol на alice: `followers alice` покажет bob и carol, `friends alice` только bob. bob лайкает пост alice, и в его `feed` у поста видно «ты лайкнул». `card alice` покажет подписчиков, подписки, посты и полученные лайки, а админ увидит `stats 7` с нулями в пустые дни.
