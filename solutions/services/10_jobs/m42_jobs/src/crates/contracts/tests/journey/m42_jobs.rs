use contracts::{
    DomainEvent, Fact, KINDS, NoticeKind, NoticeView, Push, Topics, UserDeleted, UserId,
};
use serde_json::json;

#[test]
fn jobs_live_in_their_own_streams_and_buckets() {
    let topics = Topics::new("social");
    assert_eq!(topics.jobs_stream(), "SOCIAL_JOBS");
    assert_eq!(topics.jobs(), ["social", "jobs"]);
    assert_eq!(topics.jobs_dead_stream(), "SOCIAL_JOBS_DEAD");
    assert_eq!(topics.jobs_dead(), ["social", "jobs-dead"]);
    assert_eq!(topics.leases(), "SOCIAL_LEASES");
    assert_eq!(topics.checkpoints(), "SOCIAL_CHECKPOINTS");
    assert_eq!(topics.sagas(), "SOCIAL_SAGAS");
    let events = topics.stream_subjects();
    assert!(
        !events
            .iter()
            .any(|subject| subject.starts_with("social.jobs")),
        "задачи не попадают в stream событий: у очереди задач своя WorkQueue"
    );
}

#[test]
fn a_deleted_user_is_a_fact_about_that_user() {
    assert_eq!(KINDS.len(), 11);
    let fact = Fact::UserDeleted(UserDeleted { user_id: UserId(7) });
    assert_eq!(
        (fact.kind(), fact.schema(), fact.aggregate()),
        ("user.deleted", 1, String::from("u_7"))
    );
    let event = DomainEvent::new("evt-1", &fact, 4, 9);
    assert_eq!(
        serde_json::to_value(&event).expect("JSON")["payload"],
        json!({"user_id": "u_7"})
    );
    assert_eq!(event.fact(), Ok(Some(fact)));
}

#[test]
fn a_digest_is_a_notice_and_a_push() {
    let notice = NoticeView {
        id: 3,
        to: UserId(1),
        kind: NoticeKind::Digest,
        actor: UserId(1),
        post: None,
        read: false,
        count: Some(5),
    };
    assert_eq!(
        serde_json::to_value(&notice).expect("JSON"),
        json!({"id": 3, "to": "u_1", "kind": "digest", "actor": "u_1", "read": false, "count": 5})
    );
    assert_eq!(
        notice.push(),
        Some(Push::Digest {
            to: UserId(1),
            unread: 5
        })
    );
    assert_eq!(
        serde_json::to_value(Push::Digest {
            to: UserId(1),
            unread: 5
        })
        .expect("JSON"),
        json!({"event": "digest", "data": {"to": "u_1", "unread": 5}})
    );
    let old: NoticeView = serde_json::from_value(
        json!({"id": 1, "to": "u_1", "kind": "liked", "actor": "u_2", "post": "p_7", "read": true}),
    )
    .expect("уведомление без count читается");
    assert_eq!(old.count, None);
}
