use std::borrow::Cow;

use social_core::{
    CommandError, CommentId, Cursor, DomainError, Error, MAX_COMMENT_LEN, MemNetwork, PREVIEW_LEN,
    Post, PostId, SharedNetwork, TRENDS_WINDOW, Timestamp, Trend, UserId, Visibility, execute_line,
    preview,
};

struct World {
    net: MemNetwork,
    alice: UserId,
    bob: UserId,
    carol: UserId,
}

fn world() -> World {
    let mut net = MemNetwork::new();
    let alice = net.register("alice").expect("alice регистрируется");
    let bob = net.register("bob").expect("bob регистрируется");
    let carol = net.register("carol").expect("carol регистрируется");
    net.follow(bob, alice).expect("bob подписывается на alice");
    World {
        net,
        alice,
        bob,
        carol,
    }
}

impl World {
    fn post_at(&mut self, author: UserId, text: &str, time: Timestamp) -> PostId {
        self.net
            .publish_at(author, text, Visibility::Public, time)
            .expect("публикация проходит")
    }

    fn like_at(&mut self, user: UserId, post: PostId, time: Timestamp) {
        let fresh = self.net.like_at(user, post, time).expect("лайк проходит");
        assert!(fresh, "первый лайк пользователя засчитывается");
    }

    fn page(
        &self,
        viewer: UserId,
        author: UserId,
        after: Option<Cursor>,
        size: usize,
    ) -> (Vec<PostId>, Option<Cursor>) {
        let page = self
            .net
            .author_page(viewer, author, after, size)
            .expect("страница автора открывается");
        (page.posts.iter().map(|post| post.id()).collect(), page.next)
    }

    fn all_pages(&self, viewer: UserId, author: UserId, size: usize) -> Vec<Vec<PostId>> {
        let mut pages = Vec::new();
        let mut after = None;
        for _ in 0..50 {
            let (ids, next) = self.page(viewer, author, after, size);
            pages.push(ids);
            match next {
                Some(cursor) => after = Some(cursor),
                None => return pages,
            }
        }
        panic!("страницы автора не кончаются: next не стал None за 50 запросов")
    }

    fn cursor_of(&self, post: PostId) -> Cursor {
        self.net
            .post(post)
            .map(Post::cursor)
            .expect("пост есть в хранилище")
    }

    fn thread(&self, viewer: UserId, post: PostId) -> Vec<(usize, CommentId)> {
        self.net
            .thread(viewer, post)
            .expect("ветка комментариев открывается")
            .iter()
            .map(|(depth, comment)| (*depth, comment.id()))
            .collect()
    }
}

fn trend(post: PostId, likes: usize) -> Trend {
    Trend { post, likes }
}

#[test]
fn author_page_goes_by_post_time_not_by_id() {
    let mut w = world();
    let (alice, bob) = (w.alice, w.bob);
    let late = w.post_at(alice, "поздний", 300);
    w.post_at(bob, "чужой", 250);
    let early = w.post_at(alice, "ранний", 100);
    let middle = w.post_at(alice, "средний", 200);
    assert_eq!(
        w.page(bob, alice, None, 10),
        (vec![late, middle, early], None),
        "страница автора идёт от новых к старым по времени поста, чужих постов в ней нет, а next None, когда дальше ничего"
    );
    assert_eq!(
        w.net.post(early).map(Post::time),
        Some(100),
        "publish_at запоминает время поста"
    );
    assert_eq!(
        w.cursor_of(middle),
        Cursor {
            time: 200,
            post: middle
        },
        "курсор поста это пара (время, id)"
    );
}

#[test]
fn equal_times_are_ordered_by_id_and_pages_neither_repeat_nor_lose() {
    let mut w = world();
    let (alice, bob) = (w.alice, w.bob);
    let old: Vec<PostId> = (0..3)
        .map(|i| w.post_at(alice, &format!("старый {i}"), 50))
        .collect();
    let same: Vec<PostId> = (0..7)
        .map(|i| w.post_at(alice, &format!("одновременный {i}"), 100))
        .collect();
    let expected: Vec<PostId> = same.iter().rev().chain(old.iter().rev()).copied().collect();
    let pages = w.all_pages(bob, alice, 3);
    assert_eq!(
        pages.iter().map(Vec::len).collect::<Vec<usize>>(),
        [3, 3, 3, 1],
        "10 постов страницами по 3: 3, 3, 3 и 1, без пустой страницы в конце"
    );
    assert_eq!(
        pages.concat(),
        expected,
        "при одинаковом времени новее больший id; склеенные страницы дают все посты по разу, без повторов и потерь на стыках"
    );
    let (first, next) = w.page(bob, alice, None, 3);
    assert_eq!(
        next,
        Some(w.cursor_of(first[2])),
        "next это курсор последнего поста страницы"
    );
}

#[test]
fn exact_fit_needs_no_empty_page() {
    let mut w = world();
    let (alice, bob) = (w.alice, w.bob);
    for time in 1..=6 {
        w.post_at(alice, &format!("пост {time}"), time);
    }
    let (_, next) = w.page(bob, alice, None, 3);
    let after = next.expect("за первой страницей есть ещё посты");
    let (second, next) = w.page(bob, alice, Some(after), 3);
    assert_eq!(second.len(), 3, "вторая страница полная");
    assert_eq!(
        next, None,
        "6 постов по 3: на второй странице next уже None, лишний пустой запрос не нужен"
    );
}

#[test]
fn cursor_survives_deleting_the_boundary_post() {
    let mut w = world();
    let (alice, bob) = (w.alice, w.bob);
    let ids: Vec<PostId> = (1..=5)
        .map(|time| w.post_at(alice, &format!("пост {time}"), time))
        .collect();
    let (first, next) = w.page(bob, alice, None, 2);
    assert_eq!(first, [ids[4], ids[3]], "первая страница: два самых новых");
    w.net
        .delete_post(alice, ids[3])
        .expect("автор удаляет свой пост");
    assert_eq!(
        w.page(bob, alice, next, 2),
        (vec![ids[2], ids[1]], Some(w.cursor_of(ids[1]))),
        "курсор это ключ (время, id), а не ссылка на пост: удаление поста на границе ничего не сдвигает"
    );
}

#[test]
fn author_page_shows_only_what_the_viewer_may_see() {
    let mut w = world();
    let (alice, bob, carol) = (w.alice, w.bob, w.carol);
    let public = w.post_at(alice, "всем", 1);
    let followers = w
        .net
        .publish_at(alice, "подписчикам", Visibility::Followers, 2)
        .expect("публикация проходит");
    let private = w
        .net
        .publish_at(alice, "себе", Visibility::Private, 3)
        .expect("публикация проходит");
    assert_eq!(
        w.page(alice, alice, None, 10).0,
        [private, followers, public],
        "автор видит все свои посты"
    );
    assert_eq!(
        w.page(bob, alice, None, 10).0,
        [followers, public],
        "подписчик видит публичные посты и посты для подписчиков"
    );
    assert_eq!(
        w.page(carol, alice, None, 1),
        (vec![public], None),
        "carol видит только публичный пост; next смотрит на видимые посты, поэтому дальше ничего"
    );
    w.net.mute(bob, alice).expect("bob скрывает alice");
    assert_eq!(
        w.page(bob, alice, None, 10).0,
        [followers, public],
        "mute прячет автора из ленты, но страница автора это профиль, и mute на неё не влияет"
    );
}

#[test]
fn empty_and_broken_page_requests() {
    let mut w = world();
    let (alice, bob, carol) = (w.alice, w.bob, w.carol);
    w.post_at(alice, "пост", 1);
    assert_eq!(
        w.page(bob, alice, None, 0),
        (Vec::new(), None),
        "страница размера 0 пустая, и next у неё None"
    );
    assert_eq!(
        w.page(bob, carol, None, 10),
        (Vec::new(), None),
        "у автора без постов пустая страница"
    );
    assert!(
        matches!(
            w.net.author_page(UserId(99), alice, None, 10),
            Err(DomainError::UnknownUser(UserId(99)))
        ),
        "незнакомый читатель это UnknownUser"
    );
    assert!(
        matches!(
            w.net.author_page(bob, UserId(77), None, 10),
            Err(DomainError::UnknownUser(UserId(77)))
        ),
        "незнакомый автор это UnknownUser"
    );
}

#[test]
fn deleting_a_post_cleans_every_index() {
    let mut w = world();
    let (alice, bob, carol) = (w.alice, w.bob, w.carol);
    let kept = w.post_at(alice, "Учу #Rust", 10);
    let gone = w.post_at(alice, "#rust и #go", 20);
    w.like_at(bob, gone, 30);
    w.like_at(bob, kept, 30);
    let comment = w
        .net
        .comment(carol, gone, "интересно")
        .expect("комментарий проходит");
    assert_eq!(
        w.net.delete_post(alice, gone),
        Ok(true),
        "автор удаляет свой пост"
    );
    assert_eq!(
        w.page(bob, alice, None, 10).0,
        [kept],
        "удалённого поста нет на странице автора"
    );
    assert_eq!(
        w.net.posts_with_tag("rust"),
        [kept],
        "удалённый пост уходит из индекса тегов"
    );
    assert!(
        w.net.posts_with_tag("go").is_empty(),
        "у #go не осталось постов"
    );
    assert_eq!(
        w.net.trending(40, 100, 10),
        [trend(kept, 1)],
        "удалённый пост уходит из трендов"
    );
    assert!(
        matches!(
            w.net.thread(bob, gone),
            Err(DomainError::UnknownPost(id)) if id == gone
        ),
        "ветки удалённого поста больше нет"
    );
    assert!(
        w.net.find_comment(comment).is_none(),
        "комментарии удалённого поста удаляются вместе с ним"
    );
    assert_eq!(
        w.net.reply(bob, comment, "ответ"),
        Err(DomainError::UnknownComment(comment)),
        "ответить на комментарий удалённого поста нельзя"
    );
    assert_eq!(
        w.net.delete_post(alice, gone),
        Ok(false),
        "повторное удаление ничего не меняет"
    );
    assert!(
        w.net.post(gone).is_some_and(Post::is_deleted),
        "удаление по-прежнему мягкое: пост остался в хранилище с пометкой"
    );
}

#[test]
fn trends_count_likes_inside_the_window() {
    let mut w = world();
    let (alice, bob, carol) = (w.alice, w.bob, w.carol);
    let first = w.post_at(alice, "раз", 1);
    let second = w.post_at(alice, "два", 2);
    let third = w.post_at(alice, "три", 3);
    w.like_at(bob, first, 900);
    w.like_at(carol, first, 901);
    w.like_at(bob, second, 1000);
    w.like_at(carol, second, 950);
    w.like_at(bob, third, 1001);
    assert_eq!(
        w.net.trending(1000, 100, 10),
        [trend(second, 2), trend(first, 1)],
        "окно (now - window, now]: лайк в 900 уже не считается, 901 и 1000 считаются, 1001 ещё не наступил"
    );
    assert_eq!(
        w.net.trending(1001, 100, 10),
        [trend(second, 2), trend(third, 1)],
        "через секунду лайк в 901 выпал из окна, а лайк в 1001 вошёл"
    );
    assert_eq!(
        w.net.trending(50, 1000, 10),
        [],
        "окно длиннее now не уходит ниже нуля и не паникует"
    );
    assert_eq!(TRENDS_WINDOW, 86_400, "окно трендов терминала это сутки");
}

#[test]
fn trends_sort_by_likes_then_by_older_post() {
    let mut w = world();
    let fans: Vec<UserId> = (0..5)
        .map(|i| {
            w.net
                .register(&format!("fan{i}"))
                .expect("фанат регистрируется")
        })
        .collect();
    let author = w.alice;
    let likes_per_post = [3, 3, 5, 1];
    let mut posts = Vec::new();
    for (i, likes) in likes_per_post.iter().enumerate() {
        let post = w.post_at(author, &format!("пост {i}"), 1);
        for fan in &fans[..*likes] {
            w.like_at(*fan, post, 100);
        }
        posts.push(post);
    }
    assert_eq!(
        w.net.trending(100, 50, 3),
        [trend(posts[2], 5), trend(posts[0], 3), trend(posts[1], 3)],
        "тренды по убыванию лайков, при равенстве раньше меньший id"
    );
    assert_eq!(
        w.net.trending(100, 50, 10).len(),
        4,
        "n больше числа постов"
    );
    assert_eq!(w.net.trending(100, 50, 0), [], "n = 0 даёт пустые тренды");
}

#[test]
fn top_n_keeps_the_winners() {
    let mut w = world();
    let fans: Vec<UserId> = (0..12)
        .map(|i| {
            w.net
                .register(&format!("fan{i}"))
                .expect("фанат регистрируется")
        })
        .collect();
    let author = w.alice;
    let mut expected = Vec::new();
    for k in 0..40 {
        let post = w.post_at(author, &format!("пост {k}"), k + 1);
        let likes = usize::try_from(k * 7 % 12).expect("число лайков маленькое");
        for fan in &fans[..likes] {
            w.like_at(*fan, post, 500);
        }
        if likes > 0 {
            expected.push(trend(post, likes));
        }
    }
    expected.sort_by(|a, b| b.likes.cmp(&a.likes).then(a.post.cmp(&b.post)));
    expected.truncate(5);
    assert_eq!(
        w.net.trending(500, 100, 5),
        expected,
        "топ-5 из 40 постов: в куче размера n остаются победители, а выкидывается самый слабый (нужна min-куча)"
    );
}

#[test]
fn only_public_live_posts_trend() {
    let mut w = world();
    let (alice, bob, carol) = (w.alice, w.bob, w.carol);
    let open = w.post_at(alice, "всем", 1);
    let closed = w
        .net
        .publish_at(alice, "подписчикам", Visibility::Followers, 2)
        .expect("публикация проходит");
    w.like_at(bob, closed, 50);
    w.like_at(bob, open, 50);
    assert_eq!(
        w.net.like_at(bob, open, 60),
        Ok(false),
        "повторный лайк не засчитывается"
    );
    w.like_at(carol, open, 70);
    assert_eq!(
        w.net.trending(100, 100, 10),
        [trend(open, 2)],
        "в тренды попадают только публичные посты, повторный лайк не считается"
    );
    assert_eq!(
        w.net.likes(closed),
        1,
        "лайк непубличного поста при этом засчитан"
    );
}

#[test]
fn comments_form_a_depth_first_thread() {
    let mut w = world();
    let (alice, bob, carol) = (w.alice, w.bob, w.carol);
    let post = w.post_at(alice, "Что почитать про Rust?", 1);
    let other = w.post_at(alice, "Другой пост", 2);
    let c1 = w
        .net
        .comment(bob, post, "  Книгу  ")
        .expect("комментарий проходит");
    let c2 = w.net.reply(alice, c1, "Какую?").expect("ответ проходит");
    let c3 = w
        .net
        .comment(carol, post, "Растбук")
        .expect("комментарий проходит");
    let c4 = w
        .net
        .reply(bob, c2, "Rust Book")
        .expect("ответ на ответ проходит");
    let c5 = w
        .net
        .reply(carol, c1, "Поддерживаю")
        .expect("ответ проходит");
    let elsewhere = w
        .net
        .comment(bob, other, "мимо")
        .expect("комментарий проходит");
    assert_eq!(
        w.thread(bob, post),
        [(0, c1), (1, c2), (2, c4), (1, c5), (0, c3)],
        "ветка обходится в глубину: комментарий, за ним его ответы по порядку, у каждого глубина"
    );
    assert_eq!(
        w.thread(bob, other),
        [(0, elsewhere)],
        "у каждого поста своя ветка"
    );
    let comment = w.net.find_comment(c4).expect("комментарий находится по id");
    assert_eq!(comment.post(), post, "ответ принадлежит посту родителя");
    assert_eq!(comment.author(), bob, "автор ответа запомнен");
    assert_eq!(comment.parent(), Some(c2), "у ответа есть parent");
    assert_eq!(
        w.net.find_comment(c1).map(|comment| comment.text()),
        Some("Книгу"),
        "текст комментария хранится без пробелов по краям"
    );
    assert_eq!(
        w.net.find_comment(c3).and_then(|comment| comment.parent()),
        None,
        "у комментария верхнего уровня parent None"
    );
    assert_ne!(
        c1, elsewhere,
        "id комментариев не повторяются между постами"
    );
}

#[test]
fn comment_rules() {
    let mut w = world();
    let (alice, bob, carol) = (w.alice, w.bob, w.carol);
    let post = w.post_at(alice, "пост", 1);
    assert_eq!(
        w.net.comment(UserId(99), post, "кто я"),
        Err(DomainError::UnknownUser(UserId(99))),
        "незнакомый автор комментария"
    );
    assert_eq!(
        w.net.comment(bob, PostId(42), "куда"),
        Err(DomainError::UnknownPost(PostId(42))),
        "комментарий к несуществующему посту"
    );
    assert_eq!(
        w.net.reply(bob, CommentId(7), "кому"),
        Err(DomainError::UnknownComment(CommentId(7))),
        "ответ на несуществующий комментарий"
    );
    assert_eq!(
        w.net.comment(bob, post, "   "),
        Err(DomainError::EmptyComment),
        "пустой комментарий"
    );
    assert_eq!(
        w.net.comment(bob, post, &"ы".repeat(MAX_COMMENT_LEN + 1)),
        Err(DomainError::CommentTooLong),
        "длина комментария считается в символах"
    );
    assert!(
        w.net
            .comment(bob, post, &"ы".repeat(MAX_COMMENT_LEN))
            .is_ok(),
        "комментарий ровно из MAX_COMMENT_LEN символов разрешён"
    );
    let secret = w
        .net
        .publish_at(alice, "себе", Visibility::Private, 2)
        .expect("публикация проходит");
    let friends = w
        .net
        .publish_at(alice, "своим", Visibility::Followers, 3)
        .expect("публикация проходит");
    assert_eq!(
        w.net.comment(bob, secret, "подсмотрел"),
        Err(DomainError::UnknownPost(secret)),
        "чужой личный пост для комментатора не существует"
    );
    assert!(
        matches!(w.net.thread(bob, secret), Err(DomainError::UnknownPost(id)) if id == secret),
        "и ветку чужого личного поста не открыть"
    );
    assert_eq!(
        w.net.comment(carol, friends, "я не подписана"),
        Err(DomainError::UnknownPost(friends)),
        "пост для подписчиков комментируют только подписчики и автор"
    );
    assert!(
        w.net.comment(bob, friends, "я подписан").is_ok(),
        "подписчик комментирует пост для подписчиков"
    );
    assert!(
        w.net.comment(alice, secret, "заметка себе").is_ok(),
        "автор комментирует свой личный пост"
    );
    assert_eq!(
        MAX_COMMENT_LEN, 280,
        "лимит комментария из ТЗ: 280 символов"
    );
}

#[test]
fn preview_cuts_on_a_char_boundary() {
    assert!(
        matches!(preview("коротко", 100), Cow::Borrowed("коротко")),
        "короткий текст отдаётся как есть, без копии"
    );
    assert_eq!(preview("hello", 5), "hello", "ровно max байт не режется");
    assert_eq!(
        preview("hello world", 5),
        "hello…",
        "ASCII режется по байтам"
    );
    assert_eq!(
        preview("привет", 5),
        "пр…",
        "5 байт попадают внутрь «и»: режем по границе символа левее, до 4 байт"
    );
    assert_eq!(
        preview("ab👋cd", 4),
        "ab…",
        "эмодзи занимает 4 байта и целиком не влезает"
    );
    assert_eq!(
        preview("раз два три", 8),
        "раз…",
        "пробел перед многоточием срезается"
    );
    let text = "aй👋b раз";
    for max in 0..=text.len() + 1 {
        let cut = preview(text, max);
        let head = cut.strip_suffix('…').unwrap_or(&cut);
        assert!(
            head.len() <= max && text.starts_with(head),
            "preview({text:?}, {max}) = {cut:?}: до многоточия должно остаться начало текста не длиннее {max} байт"
        );
    }
}

#[test]
fn post_card_shows_a_preview() {
    let mut w = world();
    let alice = w.alice;
    let long = "ы".repeat(200);
    let post = w.post_at(alice, &long, 1);
    let card = w.net.post(post).expect("пост сохранён");
    assert_eq!(PREVIEW_LEN, 80, "превью из ТЗ: 80 байт");
    assert_eq!(
        card.to_string(),
        format!("[{}] {}… (лайков: 0)", post.0, "ы".repeat(PREVIEW_LEN / 2)),
        "карточка поста показывает превью: 80 байт это 40 букв «ы»"
    );
    assert_eq!(card.text(), long, "сам текст поста не обрезается");
    assert_eq!(
        card.preview(),
        format!("{}…", "ы".repeat(40)),
        "Post::preview"
    );
}

#[test]
fn nicks_are_ascii_and_case_insensitive() {
    let mut net = MemNetwork::new();
    let alice = net
        .register(" Alice_42 ")
        .expect("латиница, цифры и _ проходят");
    assert_eq!(
        net.user(alice).map(|user| user.name().to_string()),
        Some(String::from("alice_42")),
        "ник хранится в нижнем регистре"
    );
    assert_eq!(
        net.register("ALICE_42"),
        Err(DomainError::NameTaken(String::from("alice_42"))),
        "ники сравниваются без учёта регистра"
    );
    assert_eq!(
        net.find_user("aLiCe_42").map(|user| user.id()),
        Some(alice),
        "поиск по нику тоже без учёта регистра"
    );
    assert_eq!(
        net.register("алиса"),
        Err(DomainError::BadNameChar('а')),
        "кириллица в нике не проходит"
    );
    assert_eq!(
        net.register("bob-smith"),
        Err(DomainError::BadNameChar('-')),
        "дефис в нике не проходит"
    );
    assert_eq!(
        DomainError::BadNameChar('я').to_string(),
        "в нике можно только a-z, 0-9 и _, а 'я' нельзя",
        "текст ошибки как у NameError::BadChar"
    );
}

#[test]
fn unknown_comment_is_not_found() {
    let error = Error::from(DomainError::UnknownComment(CommentId(3)));
    assert_eq!(
        error.code(),
        "not_found",
        "незнакомый комментарий это not_found"
    );
    assert_eq!(
        error.to_body().message,
        "запрос отклонён: нет комментария с id 3",
        "текст ошибки"
    );
}

#[test]
fn text_commands_for_pages_comments_and_trends() {
    let net = SharedNetwork::new(MemNetwork::new()).expect("сеть запускается");
    let run = |line: &str| execute_line(line, &net);
    for line in [
        "register alice",
        "register bob",
        "follow bob alice",
        "post alice раз",
        "post alice два",
        "post alice три",
    ] {
        run(line).expect("подготовка проходит");
    }
    assert_eq!(
        run("trends"),
        Ok(String::from("трендов пока нет")),
        "без лайков трендов нет"
    );
    let cursor = net
        .read(|network| network.post(PostId(2)).map(Post::cursor))
        .expect("пост 2 есть");
    assert_eq!(
        run("posts bob alice 2"),
        Ok(format!(
            "@alice [3] три (лайков: 0)\n@alice [2] два (лайков: 0)\nдальше: {cursor}"
        )),
        "posts <кто> <автор> [размер]: строки как в ленте и курсор следующей страницы"
    );
    assert_eq!(
        run(&format!("posts bob alice 2 {cursor}")),
        Ok(String::from("@alice [1] раз (лайков: 0)")),
        "вторая страница по курсору, последняя, без строки «дальше»"
    );
    assert_eq!(
        run("posts alice bob"),
        Ok(String::from("постов нет")),
        "у bob постов нет"
    );
    assert_eq!(
        run("comment bob 3 Класс"),
        Ok(String::from("комментарий [1] к посту [3]")),
        "comment <ник> <id поста> <текст>"
    );
    assert_eq!(
        run("reply alice 1 Спасибо"),
        Ok(String::from("ответ [2] на комментарий [1]")),
        "reply <ник> <id комментария> <текст>"
    );
    assert_eq!(
        run("thread bob 3"),
        Ok(String::from("[1] @bob: Класс\n  [2] @alice: Спасибо")),
        "thread <ник> <id поста>: два пробела отступа на каждый уровень"
    );
    assert_eq!(
        run("thread bob 2"),
        Ok(String::from("комментариев пока нет")),
        "у поста без комментариев"
    );
    run("like bob 3").expect("лайк проходит");
    assert_eq!(
        run("trends"),
        Ok(String::from("@alice [3] три (лайков: 1), за сутки +1")),
        "trends [N]: строка ленты и лайки за сутки"
    );
    assert_eq!(
        run("reply bob 99 эй"),
        Err(CommandError::Domain(DomainError::UnknownComment(
            CommentId(99)
        ))),
        "ответ на незнакомый комментарий"
    );
    assert_eq!(
        run("posts bob carol"),
        Err(CommandError::UnknownName(String::from("carol"))),
        "незнакомый автор по нику"
    );
}
