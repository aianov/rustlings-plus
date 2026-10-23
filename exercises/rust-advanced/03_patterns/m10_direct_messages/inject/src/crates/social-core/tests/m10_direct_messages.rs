use std::thread;

use contracts::{Command, Username};
use social_core::{
    CommandError, DomainError, MAX_MESSAGE_LEN, MemNetwork, MemPostStore, MemUserStore, Message,
    SharedNetwork, UserId, execute,
};

fn three() -> (MemNetwork, UserId, UserId, UserId) {
    let mut net = MemNetwork::new();
    let alice = net.register("alice").expect("alice регистрируется");
    let bob = net.register("bob").expect("bob регистрируется");
    let carol = net.register("carol").expect("carol регистрируется");
    (net, alice, bob, carol)
}

fn texts(messages: &[Message]) -> Vec<&str> {
    messages.iter().map(Message::text).collect()
}

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("нормальный ник должен проходить проверку")
}

fn shared() -> SharedNetwork<MemUserStore, MemPostStore> {
    SharedNetwork::new(MemNetwork::new()).expect("фоновый поток уведомлений должен запуститься")
}

#[test]
fn message_limit_is_fixed() {
    assert_eq!(MAX_MESSAGE_LEN, 500, "сообщение длиной до 500 символов");
}

#[test]
fn a_dialog_belongs_to_both_sides() {
    let (mut net, alice, bob, _) = three();
    let sent = net
        .send_message(alice, bob, "  привет  ")
        .expect("alice пишет bob")
        .clone();
    assert_eq!(
        (sent.sender(), sent.recipient(), sent.text()),
        (alice, bob, "привет"),
        "send_message отдаёт сохранённое сообщение: от кого, кому и текст с обрезанными краями"
    );
    net.send_message(bob, alice, "и тебе привет")
        .expect("bob отвечает alice");
    let seen_by_alice = net
        .history(alice, bob, 10)
        .expect("история alice и bob читается")
        .to_vec();
    let seen_by_bob = net
        .history(bob, alice, 10)
        .expect("история bob и alice читается")
        .to_vec();
    assert_eq!(
        texts(&seen_by_alice),
        ["привет", "и тебе привет"],
        "история идёт от старых сообщений к новым"
    );
    assert_eq!(
        seen_by_alice, seen_by_bob,
        "у переписки двоих одна история, с какой стороны ни смотри"
    );
    assert_eq!(
        seen_by_alice[1].sender(),
        bob,
        "второе сообщение написал bob"
    );
}

#[test]
fn history_shows_the_latest_messages_in_order() {
    let (mut net, alice, bob, _) = three();
    for n in 1..=5 {
        let (from, to) = if n % 2 == 1 {
            (alice, bob)
        } else {
            (bob, alice)
        };
        net.send_message(from, to, &format!("сообщение {n}"))
            .expect("сообщение уходит");
    }
    let last_two = net.history(alice, bob, 2).expect("история читается");
    assert_eq!(
        texts(last_two),
        ["сообщение 4", "сообщение 5"],
        "лимит берёт последние сообщения, но порядок внутри остаётся от старых к новым"
    );
    let all = net.history(bob, alice, 100).expect("история читается");
    assert_eq!(all.len(), 5, "лимит больше истории отдаёт всю историю");
    let none = net.history(alice, bob, 0).expect("история читается");
    assert!(none.is_empty(), "лимит 0 отдаёт пустую историю");
}

#[test]
fn dialogs_do_not_mix() {
    let (mut net, alice, bob, carol) = three();
    net.send_message(alice, bob, "для bob")
        .expect("сообщение уходит");
    net.send_message(carol, alice, "для alice от carol")
        .expect("сообщение уходит");
    assert_eq!(
        texts(net.history(alice, bob, 10).expect("история читается")),
        ["для bob"],
        "в переписке alice и bob только их сообщения"
    );
    assert_eq!(
        texts(net.history(alice, carol, 10).expect("история читается")),
        ["для alice от carol"],
        "в переписке alice и carol только их сообщения"
    );
    assert!(
        net.history(bob, carol, 10)
            .expect("история читается")
            .is_empty(),
        "bob и carol не переписывались: история пустая, это не ошибка"
    );
}

#[test]
fn messages_follow_the_rules() {
    let (mut net, alice, bob, _) = three();
    let ghost = UserId(99);
    let cases = [
        (
            alice,
            alice,
            String::from("привет"),
            DomainError::SelfMessage,
        ),
        (alice, bob, String::from("   "), DomainError::EmptyMessage),
        (
            alice,
            bob,
            "ы".repeat(MAX_MESSAGE_LEN + 1),
            DomainError::MessageTooLong,
        ),
        (
            alice,
            ghost,
            String::from("привет"),
            DomainError::UnknownUser(ghost),
        ),
        (
            ghost,
            bob,
            String::from("привет"),
            DomainError::UnknownUser(ghost),
        ),
        (
            ghost,
            ghost,
            String::from("привет"),
            DomainError::UnknownUser(ghost),
        ),
    ];
    for (from, to, text, error) in cases {
        assert_eq!(
            net.send_message(from, to, &text).cloned(),
            Err(error.clone()),
            "сообщение от {from:?} к {to:?} длиной {} должно дать {error:?}",
            text.chars().count()
        );
    }
    let longest = "ы".repeat(MAX_MESSAGE_LEN);
    net.send_message(alice, bob, &longest)
        .expect("ровно MAX_MESSAGE_LEN символов можно: считаются символы, а не байты");
    assert_eq!(
        net.history(alice, bob, 10).expect("история читается").len(),
        1,
        "отклонённые сообщения в историю не попадают"
    );
    assert_eq!(
        net.history(alice, ghost, 10).map(<[Message]>::len),
        Err(DomainError::UnknownUser(ghost)),
        "история с неизвестным пользователем это ошибка UnknownUser"
    );
}

#[test]
fn chat_errors_read_like_sentences() {
    let texts = [
        (DomainError::SelfMessage, "нельзя написать самому себе"),
        (DomainError::EmptyMessage, "сообщение не может быть пустым"),
        (
            DomainError::MessageTooLong,
            "сообщение длиннее 500 символов",
        ),
    ];
    for (error, text) in texts {
        assert_eq!(error.to_string(), text, "Display для {error:?}");
    }
}

#[test]
fn shared_network_carries_messages_between_threads() {
    let net = shared();
    let alice = net.register("alice").expect("alice регистрируется");
    let bob = net.register("bob").expect("bob регистрируется");
    thread::scope(|scope| {
        for writer in 0..4 {
            let net = net.clone();
            scope.spawn(move || {
                for n in 0..25 {
                    let (from, to) = if writer % 2 == 0 {
                        (alice, bob)
                    } else {
                        (bob, alice)
                    };
                    let sent = net
                        .send_message(from, to, &format!("{writer}:{n}"))
                        .expect("сообщение уходит из любого потока");
                    assert_eq!(
                        sent.text(),
                        format!("{writer}:{n}"),
                        "SharedNetwork::send_message отдаёт копию сохранённого сообщения"
                    );
                }
            });
        }
    });
    let history: Vec<String> = net
        .read(|network| {
            network
                .history(alice, bob, 100)
                .map(|messages| texts(messages).into_iter().map(String::from).collect())
        })
        .expect("история читается через read");
    assert_eq!(
        history.len(),
        100,
        "все 100 сообщений из четырёх потоков дошли"
    );
    for writer in 0..4 {
        let own: Vec<&String> = history
            .iter()
            .filter(|text| text.starts_with(&format!("{writer}:")))
            .collect();
        let expected: Vec<String> = (0..25).map(|n| format!("{writer}:{n}")).collect();
        assert_eq!(
            own,
            expected.iter().collect::<Vec<_>>(),
            "сообщения одного потока лежат в истории в том порядке, в каком он их отправил"
        );
    }
}

#[test]
fn commands_send_and_show_messages() {
    let net = shared();
    for nick in ["alice", "bob", "carol"] {
        net.register(nick).expect("пользователь регистрируется");
    }
    let send = |from: &str, to: &str, text: &str| {
        execute(
            &Command::SendMessage {
                from: name(from),
                to: name(to),
                text: String::from(text),
            },
            &net,
        )
    };
    let show = |who: &str, with: &str, limit: usize| {
        execute(
            &Command::History {
                who: name(who),
                with: name(with),
                limit,
            },
            &net,
        )
    };
    assert_eq!(
        send("alice", "bob", "привет"),
        Ok(String::from("@alice → @bob: привет")),
        "ответ на SendMessage: `@от → @кому: текст`"
    );
    assert_eq!(
        send("bob", "alice", "  как дела?  "),
        Ok(String::from("@bob → @alice: как дела?")),
        "в ответе текст в том виде, в каком он сохранён"
    );
    assert_eq!(
        show("alice", "bob", 10),
        Ok(String::from("@alice: привет\n@bob: как дела?")),
        "History печатает по строке на сообщение: `@автор: текст`"
    );
    assert_eq!(
        show("bob", "alice", 1),
        Ok(String::from("@bob: как дела?")),
        "History с лимитом печатает последние сообщения"
    );
    assert_eq!(
        show("alice", "carol", 10),
        Ok(String::from("переписки пока нет")),
        "пустая история печатается словами"
    );
    assert_eq!(
        send("alice", "ghost", "привет"),
        Err(CommandError::UnknownName(String::from("ghost"))),
        "сообщение незнакомцу: нет такого ника"
    );
    assert_eq!(
        show("ghost", "alice", 10),
        Err(CommandError::UnknownName(String::from("ghost"))),
        "история незнакомца: нет такого ника"
    );
    assert_eq!(
        send("alice", "alice", "привет"),
        Err(CommandError::Domain(DomainError::SelfMessage)),
        "самому себе писать нельзя, ошибка приходит из домена"
    );
}
