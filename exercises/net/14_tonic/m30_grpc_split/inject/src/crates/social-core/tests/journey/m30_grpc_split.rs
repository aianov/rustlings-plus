use social_core::{App, Chats, DomainError, MemNetwork, Relations, UserId};

#[tokio::test]
async fn relations_list_the_graph_around_a_user() {
    let app = App::new(MemNetwork::new()).expect("ядро стартует внутри рантайма");
    let mut ids = Vec::new();
    for name in ["alice", "bob", "carol", "dave"] {
        ids.push(app.register(name).await.expect("регистрация"));
    }
    let [alice, bob, carol, dave] = [ids[0], ids[1], ids[2], ids[3]];
    for (who, whom) in [(bob, alice), (carol, alice), (alice, dave)] {
        assert!(app.follow(who, whom).await.expect("подписка"));
    }
    assert!(app.mute(carol, alice).await.expect("скрытие"));
    assert!(app.mute(alice, carol).await.expect("скрытие"));
    assert!(app.mute(alice, bob).await.expect("скрытие"));
    let relations = app.relations(alice).await;
    assert_eq!(
        relations,
        Relations {
            following: vec![dave],
            followers: vec![bob, carol],
            muted: vec![bob, carol],
            audience: vec![bob],
        },
        "following и followers в порядке подписки, muted по возрастанию id, \
         audience: подписчики, которые не скрыли автора"
    );
    assert_eq!(
        app.relations(UserId(99)).await,
        Relations::default(),
        "у незнакомца связей нет"
    );
    app.close(std::time::Duration::from_secs(1)).await;
}

#[test]
fn chats_are_usable_outside_the_core() {
    let mut chats = Chats::default();
    let (alice, bob) = (UserId(1), UserId(2));
    let sent = chats
        .send(alice, bob, "  привет  ")
        .expect("сообщение уходит")
        .clone();
    assert_eq!(
        (sent.sender(), sent.recipient(), sent.text()),
        (alice, bob, "привет")
    );
    assert_eq!(
        chats.history(bob, alice, 10).len(),
        1,
        "диалог один на двоих"
    );
    assert!(matches!(
        chats.send(alice, alice, "я"),
        Err(DomainError::SelfMessage)
    ));
}
