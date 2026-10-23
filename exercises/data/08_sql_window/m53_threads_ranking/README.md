# Веха 53. Ветки комментариев и рейтинги

Комментарии в соцсети плоские: ответить на комментарий нельзя, а ветку из сотни ответов пришлось бы собирать в коде запросом на каждый уровень. В ленте под постом не видно ни одного комментария, а чтобы найти знакомых или лучших авторов недели, сервису пришлось бы перебрать всю базу в памяти.

Эта веха решает всё это одним запросом на задачу: ветка любой глубины это рекурсивный CTE (глубина, порядок по пути, предел глубины, защита от циклов через CYCLE, заглушки удалённых с ответами, число ответов), три последних комментария под каждым постом ленты приходят через LEFT JOIN LATERAL, «возможно, вы знакомы» собирается INTERSECT и EXCEPT, а рейтинг авторов недели считает места с ничьими (RANK) и сдвиг относительно прошлой недели. Ветки и рейтинги живут только в Postgres: без базы post-service отвечает «недоступно».

Всё собирается из глав данных: 07_sql_subqueries и 08_sql_window (задачи pgsub, pgsub_comment_thread, pgwin, pgwin_leaderboard).

## Что сделать

1. **proto**: `CommentRequest.parent_id = 4` (0 значит комментарий к посту); `Post.latest = 9` (`LatestComment { id, author_id, text }`); rpc `Thread(ThreadRequest { post_id, max_depth })` → `ThreadReply { nodes: [ThreadNode { id, parent_id, author_id, text, depth, replies, deleted }] }` и `TopAuthors(TopRequest { limit })` → `TopReply { authors: [AuthorRank { author_id, author_name, place, likes, shift }] }`; в users.proto rpc `Suggest(RelatedRequest)` → `ProfileList`.

2. **post-service**: `migrations/0002_threads.sql` (`comments.parent`, `comments.at`, индексы по родителю, по посту и по времени лайка; файл идемпотентен, как в m51); `repository/threads.rs` с трейтом `ThreadQueries` для `PgPosts`: `reply` (родитель из того же поста и не удалён, иначе `None`; факт `PostCommented` в той же транзакции), `thread` (рекурсивный CTE с `CYCLE id SET looped USING trail`, удалённый с ответами остаётся заглушкой без текста), `latest` (LATERAL с `LIMIT 3`, пост без комментариев тоже в ответе), `top` (лайки этой и прошлой недели через `FILTER`, `RANK() OVER`, сдвиг мест); `service/threads.rs` (`MAX_DEPTH`, `MAX_TOP`, `PostError::NoThreads`, `UnknownParent`); `controller/threads.rs`; `GetPosts` заполняет `latest`.

3. **user-service**: `SocialQueries::suggest`: подписки твоих подписок `INTERSECT` видимые пользователи `EXCEPT` твои подписки `EXCEPT` ты сам; в памяти то же самое; `Related::Suggested`.

4. **contracts**: `PostView.latest`, `CommentLineView`, `ThreadView { post, nodes: [CommentNodeView] }`, `TopView { authors: [RankView] }`.

5. **Шлюз**: `posts.comment` принимает `parent`; `posts.thread { post, depth? }`, `posts.top { limit? }`, `users.suggest { limit? }` для вошедших; лента приносит три последних комментария под постом.

6. **social-cli**: `comments <пост>` рисует дерево с отступами, `reply <пост> <комментарий> <текст>`, `suggest`, `top-authors`; под постом в ленте последние комментарии.

## Приёмка

Тестам нужны Postgres и NATS курса.

- `post-service/tests/journey/m53_threads_ranking.rs`: дерево в порядке пути с глубиной и числом ответов, предел глубины, чужой родитель отклонён, заглушка удалённого, три последних под каждым постом, ничья в рейтинге делит место.
- `user-service/tests/journey/m52_sql_queries.rs`: подсказки знакомых без своих подписок и себя.
- тесты прошлых вех проходят, `cargo make check` зелёный.

Архитектура: новые файлы на месте; `WITH RECURSIVE`, `CYCLE`, предел глубины, порядок по пути, `LEFT JOIN LATERAL`, `RANK() OVER`, `FILTER`, `INTERSECT` и `EXCEPT`; нет OFFSET; и все договорённости прошлых вех.

## Как запустить

```
cd backend
cargo make check
cargo make demo
```

alice пишет пост, bob комментирует, carol отвечает на его комментарий (`reply 1 1 согласна`), alice отвечает carol. `comments 1` рисует дерево, в `feed` под постом видны последние комментарии, `suggest` подсказывает знакомых, `top-authors` показывает авторов недели.
