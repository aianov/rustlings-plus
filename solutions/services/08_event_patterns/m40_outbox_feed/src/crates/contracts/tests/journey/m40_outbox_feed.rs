use contracts::{
    Cause, DomainEvent, Fact, FactError, Followed, LinkView, Origin, Outbox, PROFILE_SCHEMA,
    PostCreated, PostId, PostLiked, ProfileChanged, Registered, Topics, UserId, upcast,
};
use serde_json::{Value, json};

fn post_created(post: u64) -> Fact {
    Fact::PostCreated(PostCreated {
        post_id: PostId(post),
        author_id: UserId(1),
        author_name: String::from("alice"),
        text: String::from("привет"),
        tags: vec![String::from("rust")],
        links: vec![LinkView {
            url: String::from("https://example.com"),
            title: None,
        }],
    })
}

#[test]
fn the_envelope_is_fixed_on_the_wire() {
    let event = DomainEvent::new(
        "evt-1",
        &Fact::Followed(Followed {
            follower_id: UserId(2),
            followee_id: UserId(1),
        }),
        3,
        1_700_000_000_000_000,
    );
    assert_eq!(
        serde_json::to_value(&event).expect("JSON"),
        json!({
            "event_id": "evt-1",
            "type": "user.followed",
            "schema": 1,
            "version": 3,
            "occurred_at": 1_700_000_000_000_000_u64,
            "aggregate_id": "u_2",
            "payload": {"follower_id": "u_2", "followee_id": "u_1"}
        }),
        "конверт: идентификатор, тип, схема, версия в агрегате, время, агрегат и данные; агрегат подписки это подписчик"
    );
}

#[test]
fn the_reader_is_tolerant() {
    let raw = json!({
        "event_id": "evt-9",
        "type": "post.liked",
        "version": 2,
        "occurred_at": 5,
        "aggregate_id": "p_7",
        "trace": "лишнее поле конверта",
        "payload": {"post_id": "p_7", "author_id": "u_1", "user_id": "u_2", "emoji": "🔥"}
    });
    let event: DomainEvent = serde_json::from_value(raw).expect("лишние поля не мешают");
    assert_eq!(event.schema, 1, "нет schema, значит 1");
    assert_eq!(
        event.fact(),
        Ok(Some(Fact::PostLiked(PostLiked {
            post_id: PostId(7),
            author_id: UserId(1),
            user_id: UserId(2)
        }))),
        "лишнее поле в данных пропускается"
    );
    let mut future = event.clone();
    future.kind = String::from("post.pinned");
    assert_eq!(
        future.fact(),
        Ok(None),
        "незнакомый тип это не ошибка: его просто никто не ждёт"
    );
    let mut hollow = event;
    hollow.payload = json!({"post_id": "p_7"});
    assert!(
        matches!(hollow.fact(), Err(FactError::BadPayload { .. })),
        "знакомый тип без нужного поля это ошибка, а не тихий ноль"
    );
}

#[test]
fn a_profile_change_of_schema_one_is_lifted_to_two() {
    let v1 = |payload: Value| DomainEvent {
        event_id: String::from("old"),
        kind: String::from("user.profile_changed"),
        schema: 1,
        version: 2,
        occurred_at: 9,
        aggregate_id: String::from("u_4"),
        payload,
    };
    let lifted = upcast(v1(json!({"name": "  Анна Мария ", "color": "blue"}))).expect("апкаст");
    assert_eq!(lifted.schema, PROFILE_SCHEMA);
    assert_eq!(
        lifted.payload,
        json!({"user_id": "u_4", "display_name": "Анна Мария", "color": "blue"}),
        "name стал display_name без пробелов по краям, user_id взят из агрегата, чужие поля целы"
    );
    assert_eq!(
        v1(json!({"name": ""})).fact(),
        Ok(Some(Fact::ProfileChanged(ProfileChanged {
            user_id: UserId(4),
            display_name: None
        }))),
        "пустое имя в схеме 1 значило «без имени»"
    );
    assert!(
        matches!(upcast(v1(json!({}))), Err(FactError::BadPayload { .. })),
        "в схеме 1 без name нечего поднимать"
    );
    let mut v3 = v1(json!({"display_name": "x"}));
    v3.schema = 3;
    assert!(
        matches!(upcast(v3), Err(FactError::UnknownSchema { schema: 3, .. })),
        "схема из будущего это честная ошибка"
    );
    let liked = DomainEvent::new(
        "e",
        &Fact::PostLiked(PostLiked {
            post_id: PostId(1),
            author_id: UserId(1),
            user_id: UserId(2),
        }),
        1,
        1,
    );
    assert_eq!(
        upcast(liked.clone()),
        Ok(liked),
        "остальные типы апкаст не трогает"
    );
}

#[test]
fn every_fact_survives_the_envelope_and_names_its_aggregate() {
    let facts = [
        (
            Fact::Registered(Registered {
                user_id: UserId(1),
                username: String::from("alice"),
            }),
            "u_1",
        ),
        (
            Fact::ProfileChanged(ProfileChanged {
                user_id: UserId(1),
                display_name: Some(String::from("Алиса")),
            }),
            "u_1",
        ),
        (
            Fact::Followed(Followed {
                follower_id: UserId(2),
                followee_id: UserId(1),
            }),
            "u_2",
        ),
        (post_created(7), "p_7"),
    ];
    for (fact, aggregate) in facts {
        let event = DomainEvent::new("e", &fact, 1, 1);
        assert_eq!(event.aggregate_id, aggregate, "{fact:?}");
        assert_eq!(event.fact(), Ok(Some(fact)), "туда и обратно без потерь");
    }
    let topics = Topics::new("social");
    let event = DomainEvent::new("e", &post_created(7), 1, 1);
    assert_eq!(
        topics.event(&event).join("."),
        "social.events.post.created.p_7",
        "тема: корень, events, сущность, действие и агрегат"
    );
    assert_eq!(topics.stream(), "SOCIAL_EVENTS");
}

#[test]
fn the_outbox_numbers_rows_and_versions_per_aggregate() {
    let mut outbox = Outbox::new();
    let cause = Cause::new(&Origin::new("req-1"), 42);
    let mut rows = Vec::new();
    for fact in [post_created(1), post_created(2)] {
        let row = outbox.prepare(&fact, &cause);
        outbox.push(row.clone());
        rows.push(row);
    }
    let like = Fact::PostLiked(PostLiked {
        post_id: PostId(1),
        author_id: UserId(1),
        user_id: UserId(2),
    });
    let third = outbox.prepare(&like, &cause);
    outbox.push(third.clone());
    rows.push(third);
    let seen: Vec<(u64, String, u64, u64)> = rows
        .iter()
        .map(|row| {
            (
                row.seq,
                row.event.aggregate_id.clone(),
                row.event.version,
                row.event.occurred_at,
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            (1, String::from("p_1"), 1, 42),
            (2, String::from("p_2"), 1, 42),
            (3, String::from("p_1"), 2, 42),
        ],
        "seq сквозной, version своя у каждого агрегата, время из причины"
    );
    assert!(
        rows[0].event.event_id != rows[1].event.event_id,
        "у каждого события свой идентификатор"
    );
    assert_eq!(outbox.unpublished(10).len(), 3);
    outbox.published_through(2);
    assert_eq!(
        outbox
            .unpublished(10)
            .iter()
            .map(|row| row.seq)
            .collect::<Vec<_>>(),
        [3],
        "отметка «опубликовано до 2» снимает две первые строки"
    );
    let mut restored = Outbox::new();
    for row in rows {
        restored.push(row);
    }
    restored.published_through(2);
    assert_eq!(
        (restored.pending(), restored.version("p_1")),
        (1, 2),
        "из журнала outbox восстанавливается с версиями и отметкой"
    );
}
