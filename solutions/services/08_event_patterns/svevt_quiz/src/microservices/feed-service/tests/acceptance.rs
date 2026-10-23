use events::{Event, Journal};
use feed_service::{Consumed, FeedError, FeedService};
use serde_json::json;
use std::sync::{Arc, Barrier};
use std::time::Duration;
use user_service::{Db, DbError, SeqIds, UserError, UserService, relay_once};

struct World {
    db: Arc<Db>,
    users: UserService,
    journal: Journal,
}

fn world() -> World {
    let db = Arc::new(Db::new());
    World { users: UserService::new(db.clone(), Arc::new(SeqIds::new())), db, journal: Journal::new() }
}

impl World {
    fn publish(&self) {
        relay_once(&self.db, &self.journal, 1000).expect("релей отправил outbox");
    }

    fn post(&self, id: &str, author: &str, at: u64) {
        let event = Event::new(format!("post-evt-{id}"), "post.created", 1, 1, at, id, json!({"post_id": id, "author_id": author, "text": format!("текст {id}")}));
        self.journal.append(&event).unwrap();
    }

    fn all_events(&self) -> Vec<Event> {
        self.journal.read_from(1).into_iter().map(|(_, event)| event).collect()
    }

    fn story(&self) {
        self.users.register(10, "alice", "Alice").unwrap();
        self.users.register(11, "bob", "Bob").unwrap();
        self.users.follow(12, "alice", "bob").unwrap();
        self.publish();
        self.post("p1", "bob", 20);
        self.post("p2", "bob", 30);
        self.post("p3", "alice", 30);
        self.users.rename(40, "bob", "Robert", "Paulson").unwrap();
        self.publish();
    }
}

fn consume_all(feed: &FeedService, events: &[Event]) {
    for event in events {
        feed.consume(event).expect("событие разобрано");
    }
}

#[test]
fn registrations_follows_and_posts_build_the_feed() {
    let w = world();
    w.users.register(10, "alice", "Alice").unwrap();
    w.users.register(11, "bob", "Bob").unwrap();
    w.users.follow(12, "alice", "bob").unwrap();
    w.publish();
    w.post("p1", "bob", 20);
    w.post("p2", "alice", 25);
    let feed = FeedService::new();
    consume_all(&feed, &w.all_events());
    let items = feed.feed("alice", 10);
    let ids: Vec<&str> = items.iter().map(|i| i.post_id.as_str()).collect();
    assert_eq!(ids, vec!["p2", "p1"], "в ленте Алисы её пост и пост Боба, новые первыми");
    assert_eq!(items[1].author_name, "Bob", "имя автора берётся из проекции");
    assert!(feed.feed("bob", 10).iter().all(|i| i.author_id == "bob"), "Боб ни на кого не подписан: видит только свои");
    assert_eq!(feed.followers("bob"), 1, "у Боба один подписчик");
}

#[test]
fn a_rename_shows_up_in_old_posts() {
    let w = world();
    w.story();
    let feed = FeedService::new();
    consume_all(&feed, &w.all_events());
    let names: Vec<String> = feed.feed("alice", 10).into_iter().filter(|i| i.author_id == "bob").map(|i| i.author_name).collect();
    assert_eq!(names, vec!["Robert Paulson", "Robert Paulson"], "оба старых поста Боба показывают новое имя");
}

#[test]
fn an_event_in_the_old_schema_is_read_through_the_upcast() {
    let w = world();
    w.users.register(10, "bob", "Bob").unwrap();
    w.publish();
    let legacy = Event::new("legacy-1".into(), "user.profile_changed", 1, 2, 15, "bob", json!({"name": "Rob Pauls"}));
    w.journal.append(&legacy).unwrap();
    w.post("p1", "bob", 20);
    let feed = FeedService::new();
    consume_all(&feed, &w.all_events());
    assert_eq!(feed.feed("bob", 5)[0].author_name, "Rob Pauls", "старое событие поднято до новой схемы");
}

#[test]
fn every_event_delivered_twice_changes_nothing() {
    let w = world();
    w.story();
    let events = w.all_events();
    let once = FeedService::new();
    consume_all(&once, &events);
    let twice = FeedService::new();
    for event in &events {
        twice.consume(event).unwrap();
        assert_eq!(twice.consume(event).unwrap(), Consumed::Duplicate, "повтор события {} должен быть узнан", event.event_id);
    }
    assert_eq!(twice.snapshot(), once.snapshot(), "повторы не меняют проекцию");
    assert_eq!(twice.followers("bob"), 1, "подписчик один");
}

#[test]
fn any_arrival_order_gives_the_same_projection() {
    let w = world();
    w.story();
    let events = w.all_events();
    let in_order = FeedService::new();
    consume_all(&in_order, &events);
    let mut reversed = events.clone();
    reversed.reverse();
    let mut rotated = events.clone();
    rotated.rotate_left(3);
    for (name, order) in [("задом наперёд", reversed), ("со сдвигом", rotated)] {
        let feed = FeedService::new();
        consume_all(&feed, &order);
        assert_eq!(feed.snapshot(), in_order.snapshot(), "порядок {name}: итоговая проекция должна совпасть с проекцией при правильном порядке");
        assert_eq!(feed.snapshot().waiting, 0, "порядок {name}: после всех событий ничто не ждёт");
    }
}

#[test]
fn an_event_from_the_future_waits_for_its_predecessors() {
    let w = world();
    w.story();
    let events = w.all_events();
    let follow = events.iter().find(|e| e.kind == "user.followed").unwrap();
    let feed = FeedService::new();
    assert_eq!(feed.consume(follow).unwrap(), Consumed::Buffered, "подписка alice идёт после её регистрации: пока ждёт");
    assert_eq!(feed.consume(follow).unwrap(), Consumed::Duplicate, "повтор ожидающего это дубль");
    assert!(feed.feed("alice", 5).is_empty() && feed.followers("bob") == 0, "ожидающее событие ещё не применено");
    let registered = events.iter().find(|e| e.kind == "user.registered" && e.aggregate_id == "alice").unwrap();
    assert_eq!(feed.consume(registered).unwrap(), Consumed::Applied, "регистрация применилась");
    assert_eq!(feed.followers("bob"), 1, "и вместе с ней освободилась подписка");
}

#[test]
fn unknown_events_are_skipped_and_broken_ones_are_errors() {
    let feed = FeedService::new();
    let blocked = Event::new("x1".into(), "user.blocked", 1, 1, 5, "u1", json!({"anything": 1}));
    assert_eq!(feed.consume(&blocked).unwrap(), Consumed::Ignored, "незнакомый тип пропускается");
    let no_text = Event::new("x2".into(), "post.created", 1, 1, 5, "p1", json!({"post_id": "p1", "author_id": "a"}));
    assert!(matches!(feed.consume(&no_text), Err(FeedError::BadPayload(_))), "у поста нет текста");
    let future = Event::new("x3".into(), "user.profile_changed", 3, 1, 5, "u1", json!({}));
    assert!(matches!(feed.consume(&future), Err(FeedError::Upcast(_))), "схема из будущего");
    assert_eq!(feed.snapshot().waiting, 0, "ошибочные события не оседают в очереди ожидания");
}

#[test]
fn simultaneous_deliveries_of_one_event_are_applied_once() {
    let w = world();
    w.story();
    let events = w.all_events();
    let follow = events.iter().find(|e| e.kind == "user.followed").unwrap().clone();
    for round in 0..5 {
        let feed = Arc::new(FeedService::new());
        for event in events.iter().filter(|e| e.kind == "user.registered") {
            feed.consume(event).unwrap();
        }
        let barrier = Arc::new(Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let (feed, barrier, follow) = (feed.clone(), barrier.clone(), follow.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    feed.consume(&follow).unwrap()
                })
            })
            .collect();
        let applied = handles.into_iter().map(|h| h.join().unwrap()).filter(|c| *c == Consumed::Applied).count();
        assert_eq!(applied, 1, "раунд {round}: событие применено {applied} раз при одновременной доставке");
        assert_eq!(feed.followers("bob"), 1, "раунд {round}: подписчик один");
    }
}

#[test]
fn a_rebuild_from_the_journal_equals_the_live_projection() {
    let w = world();
    w.story();
    let events = w.all_events();
    let mut shuffled = events.clone();
    shuffled.reverse();
    let live = FeedService::new();
    for event in &shuffled {
        live.consume(event).unwrap();
        std::thread::sleep(Duration::from_millis(2));
    }
    let rebuilt = FeedService::rebuild(&w.journal).expect("пересборка");
    assert_eq!(rebuilt.snapshot(), live.snapshot(), "пересборка должна дать то же состояние");
    std::thread::sleep(Duration::from_millis(20));
    let again = FeedService::rebuild(&w.journal).unwrap();
    assert_eq!(again.feed("alice", 10), rebuilt.feed("alice", 10), "и через время тоже: проекция не зависит от часов");
    assert_eq!(rebuilt.feed("alice", 10)[0].created_at, 30, "время поста это время события");
}

#[test]
fn registering_and_renaming_write_the_event_with_the_data() {
    let w = world();
    w.users.register(10, "alice", " Alice ").unwrap();
    w.users.rename(20, "alice", "Alice", "Liddell").unwrap();
    w.users.register(30, "bob", "Bob").unwrap();
    w.users.follow(40, "alice", "bob").unwrap();
    let tables = w.db.snapshot();
    assert_eq!(tables.users.len(), 2, "два пользователя");
    let summary: Vec<(String, String, u32, u64)> = tables.outbox.iter().map(|r| (r.event.kind.clone(), r.event.aggregate_id.clone(), r.event.schema, r.event.version)).collect();
    assert_eq!(
        summary,
        vec![
            ("user.registered".to_string(), "alice".to_string(), 1, 1),
            ("user.profile_changed".to_string(), "alice".to_string(), 2, 2),
            ("user.registered".to_string(), "bob".to_string(), 1, 1),
            ("user.followed".to_string(), "alice".to_string(), 1, 3),
        ],
        "тип, агрегат, схема и версия: версии растут отдельно у каждого пользователя, подписку пишем на подписчика"
    );
    assert_eq!(tables.outbox[0].event.payload, json!({"user_id": "alice", "name": "Alice"}), "имя без пробелов по краям");
    assert_eq!(tables.outbox.iter().map(|r| r.seq).collect::<Vec<_>>(), vec![1, 2, 3, 4], "seq сквозной");
}

#[test]
fn the_rules_of_the_user_service_hold() {
    let w = world();
    w.users.register(1, "alice", "Alice").unwrap();
    w.users.register(1, "bob", "Bob").unwrap();
    assert_eq!(w.users.register(2, "alice", "Другая"), Err(UserError::AlreadyExists), "ник занят");
    assert_eq!(w.users.register(2, "carol", "   "), Err(UserError::Invalid("name")), "пустое имя");
    assert_eq!(w.users.follow(3, "alice", "alice"), Err(UserError::SelfFollow), "на себя нельзя");
    assert_eq!(w.users.follow(3, "alice", "ghost"), Err(UserError::NotFound), "на несуществующего нельзя");
    assert_eq!(w.users.rename(3, "ghost", "A", "B"), Err(UserError::NotFound), "переименовать несуществующего нельзя");
    assert_eq!(w.users.follow(4, "alice", "bob"), Ok(true), "подписка записана");
    assert_eq!(w.users.follow(5, "alice", "bob"), Ok(false), "повторная подписка не ошибка");
    assert_eq!(w.db.snapshot().outbox.len(), 3, "событий ровно три: две регистрации и одна подписка");
}

#[test]
fn a_failed_write_leaves_neither_the_row_nor_the_event() {
    let w = world();
    w.db.fail_commit_number(1);
    assert_eq!(w.users.register(1, "alice", "Alice"), Err(UserError::Db(DbError::CommitFailed)), "ошибка базы");
    let tables = w.db.snapshot();
    assert!(tables.users.is_empty() && tables.outbox.is_empty(), "ни пользователя, ни события");
    let w = world();
    w.db.fail_commit_number(2);
    assert_eq!(w.users.register(1, "alice", "Alice"), Ok(()), "запись должна быть одной транзакцией, а коммит номер два не наступает");
    let tables = w.db.snapshot();
    assert_eq!((tables.users.len(), tables.outbox.len()), (1, 1), "строка и событие вместе");
}

#[test]
fn a_relay_crash_before_the_mark_does_not_duplicate_or_lose() {
    let w = world();
    w.users.register(1, "alice", "Alice").unwrap();
    w.users.register(2, "bob", "Bob").unwrap();
    w.users.follow(3, "alice", "bob").unwrap();
    w.db.fail_next_marks(1);
    assert!(relay_once(&w.db, &w.journal, 100).is_err(), "пометку записать не удалось");
    assert_eq!(w.journal.len(), 1, "первое событие уже в журнале");
    assert_eq!(relay_once(&w.db, &w.journal, 100), Ok(3), "после перезапуска релей повторяет неотмеченное");
    assert_eq!(w.journal.len(), 3, "повтор не создал второй копии: у события тот же идентификатор");
    let kinds: Vec<String> = w.all_events().into_iter().map(|e| e.kind).collect();
    assert_eq!(kinds, vec!["user.registered", "user.registered", "user.followed"], "порядок как в outbox");
    assert_eq!(relay_once(&w.db, &w.journal, 100), Ok(0), "больше нечего отправлять");
}

#[test]
fn a_failed_publication_keeps_the_row_pending_and_the_order() {
    let w = world();
    for (i, id) in ["a", "b", "c"].iter().enumerate() {
        w.users.register(i as u64, id, id).unwrap();
    }
    w.journal.fail_next_appends(1);
    assert!(relay_once(&w.db, &w.journal, 100).is_err(), "журнал недоступен");
    assert_eq!(w.db.pending(10).len(), 3, "ничего не помечено отправленным: событие, которое не опубликовано, нельзя помечать");
    assert_eq!(relay_once(&w.db, &w.journal, 100), Ok(3), "потом уходит всё");
    let ids: Vec<String> = w.all_events().into_iter().map(|e| e.aggregate_id).collect();
    assert_eq!(ids, vec!["a", "b", "c"], "порядок сохранён");
    assert_eq!(relay_once(&w.db, &w.journal, 2), Ok(0), "ничего нового");
}
