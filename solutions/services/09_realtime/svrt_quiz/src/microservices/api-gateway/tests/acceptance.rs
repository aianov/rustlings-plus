use api_gateway::{Connection, Gateway, Out, Presence, Refused, Shared};
use bus::{Bus, typing_subject, user_subject};
use chat_service::{ChatError, ChatService};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

struct Env {
    bus: Bus,
    chat: ChatService,
    presence: Shared,
}

fn env() -> Env {
    let bus = Bus::new();
    Env { chat: ChatService::new(bus.clone()), bus, presence: Presence::shared(30_000) }
}

impl Env {
    fn gateway(&self, id: &str) -> Gateway {
        Gateway::new(id, self.bus.clone(), self.presence.clone())
    }

    fn online(&self, user: &str, now: u64) -> bool {
        self.presence.lock().unwrap().is_online(user, now)
    }
}

fn messages(connection: &Connection) -> Vec<(u64, String)> {
    connection
        .drain()
        .into_iter()
        .filter_map(|out| match out {
            Out::Message { seq, text } => Some((seq, text)),
            Out::Typing(_) => None,
        })
        .collect()
}

#[test]
fn a_message_reaches_every_device_on_every_gateway_exactly_once() {
    let env = env();
    let (a, b) = (env.gateway("gw-a"), env.gateway("gw-b"));
    let phone = a.connect(0, "alice", "s1", None).unwrap();
    let laptop = b.connect(0, "alice", "s2", None).unwrap();
    let bob = a.connect(0, "bob", "s3", None).unwrap();
    let seq = env.chat.send("carol", "alice", "привет").unwrap();
    a.pump();
    b.pump();
    a.pump();
    assert_eq!(messages(&phone), vec![(seq, "carol: привет".to_string())], "телефон на шлюзе A получил сообщение один раз");
    assert_eq!(messages(&laptop), vec![(seq, "carol: привет".to_string())], "ноутбук на шлюзе B тоже один раз");
    assert!(messages(&bob).is_empty(), "bob чужого не получает");
}

#[test]
fn a_gateway_holds_one_subscription_per_user_while_any_device_is_connected() {
    let env = env();
    let (a, b) = (env.gateway("gw-a"), env.gateway("gw-b"));
    let subject = user_subject("alice");
    let phone = a.connect(0, "alice", "s1", None).unwrap();
    let laptop = a.connect(0, "alice", "s2", None).unwrap();
    assert_eq!(env.bus.subscribers(&subject), 1, "два устройства на одном шлюзе это одна подписка");
    let tablet = b.connect(0, "alice", "s3", None).unwrap();
    assert_eq!(env.bus.subscribers(&subject), 2, "второй шлюз добавляет свою");
    a.disconnect(&phone);
    assert_eq!(env.bus.subscribers(&subject), 2, "на A остался ноутбук, подписка нужна");
    a.disconnect(&laptop);
    assert_eq!(env.bus.subscribers(&subject), 1, "на A устройств не осталось: подписка снята");
    assert_eq!(env.bus.subscribers(&typing_subject("alice")), 1, "и подписка на «печатает» тоже");
    b.disconnect(&tablet);
    assert_eq!(env.bus.subscribers(&subject) + env.bus.subscribers(&typing_subject("alice")), 0, "никого нет, подписок нет");
}

#[test]
fn typing_is_live_only() {
    let env = env();
    let a = env.gateway("gw-a");
    let alice = a.connect(0, "alice", "s1", None).unwrap();
    env.chat.typing("bob", "alice").unwrap();
    a.pump();
    assert_eq!(alice.drain(), vec![Out::Typing("bob".to_string())], "«печатает» дошло живому устройству");
    assert_eq!(env.bus.stored(), 0, "и нигде не сохранено");
    let late = a.connect(1, "alice", "s2", Some(0)).unwrap();
    assert!(late.drain().is_empty(), "опоздавшее устройство «печатает» не получает и в истории его нет");
}

#[test]
fn a_returning_device_catches_up_in_order_and_then_continues_without_repeats() {
    let env = env();
    let a = env.gateway("gw-a");
    let first = env.chat.send("bob", "alice", "m1").unwrap();
    let second = env.chat.send("bob", "alice", "m2").unwrap();
    env.chat.send("carol", "dave", "чужое").unwrap();
    let third = env.chat.send("bob", "alice", "m3").unwrap();
    let fourth = env.chat.send("bob", "alice", "m4").unwrap();
    assert!(first < second && second < third && third < fourth, "номера растут");
    let phone = a.connect(10, "alice", "s1", Some(second)).unwrap();
    assert_eq!(
        messages(&phone),
        vec![(third, "bob: m3".to_string()), (fourth, "bob: m4".to_string())],
        "после номера {second} клиент пропустил два своих сообщения, чужое из чата carol и dave ему не нужно"
    );
    a.pump();
    assert!(messages(&phone).is_empty(), "лишних повторов после пампа нет");
    let fifth = env.chat.send("bob", "alice", "m5").unwrap();
    a.pump();
    assert_eq!(messages(&phone), vec![(fifth, "bob: m5".to_string())], "живое сообщение приходит один раз");
}

#[test]
fn messages_sent_while_nobody_was_online_wait_in_the_history() {
    let env = env();
    for text in ["один", "два", "три"] {
        env.chat.send("bob", "alice", text).unwrap();
    }
    assert_eq!(env.bus.stored(), 3, "сообщения сохранены, хотя читателей нет");
    let a = env.gateway("gw-a");
    let phone = a.connect(0, "alice", "s1", Some(0)).unwrap();
    let texts: Vec<String> = messages(&phone).into_iter().map(|(_, text)| text).collect();
    assert_eq!(texts, vec!["bob: один", "bob: два", "bob: три"], "с нуля приходит вся история по порядку");
}

#[test]
fn a_message_that_arrives_while_the_history_is_read_is_neither_lost_nor_doubled() {
    let env = env();
    let a = env.gateway("gw-a");
    env.chat.send("bob", "alice", "до").unwrap();
    env.chat.send("bob", "alice", "ещё до").unwrap();
    let fired = Arc::new(AtomicBool::new(false));
    let chat = ChatService::new(env.bus.clone());
    let flag = fired.clone();
    env.bus.set_history_hook(move || {
        if !flag.swap(true, Ordering::SeqCst) {
            chat.send("bob", "alice", "во время").unwrap();
        }
    });
    let phone = a.connect(0, "alice", "s1", Some(0)).unwrap();
    assert!(fired.load(Ordering::SeqCst), "история запрашивалась, а значит сообщение «во время» написано");
    let texts: Vec<String> = messages(&phone).into_iter().map(|(_, text)| text).collect();
    assert_eq!(texts, vec!["bob: до", "bob: ещё до", "bob: во время"], "ничего не потеряно, порядок по номерам");
    a.pump();
    assert!(messages(&phone).is_empty(), "и повторов нет");
}

#[test]
fn presence_follows_the_devices_and_survives_a_crashed_gateway() {
    let env = env();
    let a = env.gateway("gw-a");
    let b = env.gateway("gw-b");
    let phone = a.connect(0, "alice", "s1", None).unwrap();
    let laptop = b.connect(0, "alice", "s2", None).unwrap();
    assert!(env.online("alice", 0), "alice в сети");
    for now in [10_000, 20_000] {
        a.tick(now);
        b.tick(now);
    }
    drop(a);
    drop(phone);
    for now in [30_000, 40_000, 50_000] {
        b.tick(now);
    }
    assert!(env.online("alice", 55_000), "шлюз A упал, но ноутбук на B продолжает слать сигналы");
    b.disconnect(&laptop);
    assert!(!env.online("alice", 55_000), "закрыто последнее живое устройство: не в сети сразу");
}

#[test]
fn a_crashed_gateway_users_disappear_after_the_ttl() {
    let env = env();
    let a = env.gateway("gw-a");
    let _phone = a.connect(0, "alice", "s1", None).unwrap();
    a.tick(20_000);
    drop(a);
    assert!(env.online("alice", 49_999), "последний сигнал в 20 с, жив до 50 с");
    assert!(!env.online("alice", 50_000), "шлюз молчит с 20 с: через ttl alice не в сети");
}

#[test]
fn closing_one_of_two_devices_on_different_gateways_keeps_the_user_online() {
    let env = env();
    let (a, b) = (env.gateway("gw-a"), env.gateway("gw-b"));
    let phone = a.connect(0, "alice", "s1", None).unwrap();
    let _laptop = b.connect(0, "alice", "s2", None).unwrap();
    assert_eq!(phone.id, 1, "номера соединений у каждого шлюза свои");
    a.disconnect(&phone);
    assert!(env.online("alice", 1_000), "ноутбук на другом шлюзе с тем же номером соединения остался");
}

#[test]
fn a_logout_closes_the_session_on_all_gateways_and_it_cannot_return() {
    let env = env();
    let (a, b) = (env.gateway("gw-a"), env.gateway("gw-b"));
    let phone = a.connect(0, "alice", "s1", None).unwrap();
    let laptop = b.connect(0, "alice", "s1", None).unwrap();
    let tablet = a.connect(0, "alice", "s2", None).unwrap();
    a.logout("s1");
    assert!(phone.is_closed(), "на своём шлюзе соединение закрыто сразу");
    assert!(!laptop.is_closed(), "второй шлюз ещё не обработал отзыв");
    b.pump();
    assert!(laptop.is_closed(), "после обработки отзыва закрыто и на втором шлюзе");
    assert!(!tablet.is_closed(), "другая сессия осталась");
    assert_eq!(a.connect(1, "alice", "s1", None).err(), Some(Refused::Revoked), "на A отозванная сессия не возвращается");
    assert_eq!(b.connect(1, "alice", "s1", None).err(), Some(Refused::Revoked), "и на B тоже");
    assert!(b.connect(1, "alice", "s9", None).is_ok(), "новая сессия допустима");
}

#[test]
fn a_revoked_connection_leaves_no_trace_in_presence_or_subscriptions() {
    let env = env();
    let a = env.gateway("gw-a");
    let _phone = a.connect(0, "alice", "s1", None).unwrap();
    a.logout("s1");
    assert!(!env.online("alice", 1), "закрытое соединение не держит пользователя в сети");
    assert_eq!(env.bus.subscribers(&user_subject("alice")), 0, "и подписка снята");
}

#[test]
fn hostile_input_is_refused_on_both_services() {
    let env = env();
    let a = env.gateway("gw-a");
    assert_eq!(a.connect(0, "a.b", "s1", None).err(), Some(Refused::BadUser), "ник с точкой");
    assert_eq!(a.connect(0, "*", "s1", None).err(), Some(Refused::BadUser), "звёздочка");
    assert_eq!(a.connect(0, "alice", "", None).err(), Some(Refused::BadSession), "пустая сессия");
    assert_eq!(env.chat.send("bob", "al ice", "x"), Err(ChatError::BadUser), "получатель с пробелом");
    assert_eq!(env.chat.send("bob", "alice", "   "), Err(ChatError::BadText), "пустой текст");
    assert_eq!(env.chat.send("bob", "alice", &"я".repeat(501)), Err(ChatError::BadText), "слишком длинный текст");
    assert!(env.chat.send("bob", "alice", &"я".repeat(500)).is_ok(), "ровно 500 символов можно");
    assert_eq!(env.chat.typing("b.b", "alice"), Err(ChatError::BadUser), "отправитель «печатает» с точкой");
    assert_eq!(env.bus.stored(), 1, "в историю попало только одно годное сообщение");
}

#[test]
fn message_text_is_trimmed_before_it_is_stored() {
    let env = env();
    env.chat.send("bob", "alice", "  привет \n").unwrap();
    let stored = env.bus.history_after(&user_subject("alice"), 0);
    assert_eq!(stored.len(), 1, "сообщение записано один раз");
    assert_eq!(stored[0].text, "bob: привет", "пробелы по краям текста обрезаются до записи, в журнале лежит чистый текст");
}
