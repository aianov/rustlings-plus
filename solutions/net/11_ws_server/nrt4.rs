// В чате появляются комнаты: клиент заходит в комнату, выходит, пишет в неё, а все остальные участники получают сообщение.
// Сообщение не должно возвращаться отправителю (он и так знает, что написал), а писать в комнату, где тебя нет, нельзя.
// TODO: Допиши Rooms. Следи за уборкой: когда соединение пропало или комната опустела, от них не должно оставаться записей.

#![allow(dead_code)]
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

type ConnId = u64;

#[derive(Default)]
struct State {
    senders: HashMap<ConnId, mpsc::Sender<String>>,
    rooms: HashMap<String, BTreeSet<ConnId>>,
}

#[derive(Clone, Default)]
struct Rooms {
    state: Arc<Mutex<State>>,
}

impl Rooms {
    fn add(&self, conn: ConnId, tx: mpsc::Sender<String>) {
        self.state.lock().unwrap().senders.insert(conn, tx);
    }

    fn join(&self, conn: ConnId, room: &str) -> bool {
        let mut state = self.state.lock().unwrap();
        if !state.senders.contains_key(&conn) {
            return false;
        }
        state.rooms.entry(room.to_string()).or_default().insert(conn)
    }

    fn leave(&self, conn: ConnId, room: &str) -> bool {
        let mut state = self.state.lock().unwrap();
        let Some(members) = state.rooms.get_mut(room) else {
            return false;
        };
        let was = members.remove(&conn);
        if members.is_empty() {
            state.rooms.remove(room);
        }
        was
    }

    fn remove(&self, conn: ConnId) {
        let mut state = self.state.lock().unwrap();
        state.senders.remove(&conn);
        state.rooms.retain(|_, members| {
            members.remove(&conn);
            !members.is_empty()
        });
    }

    fn members(&self, room: &str) -> Vec<ConnId> {
        self.state.lock().unwrap().rooms.get(room).map(|m| m.iter().copied().collect()).unwrap_or_default()
    }

    fn broadcast(&self, room: &str, from: ConnId, message: &str) -> usize {
        let targets: Vec<mpsc::Sender<String>> = {
            let state = self.state.lock().unwrap();
            let Some(members) = state.rooms.get(room) else {
                return 0;
            };
            if !members.contains(&from) {
                return 0;
            }
            members.iter().filter(|&&id| id != from).filter_map(|id| state.senders.get(id).cloned()).collect()
        };
        targets.iter().filter(|tx| tx.try_send(message.to_string()).is_ok()).count()
    }

    fn room_count(&self) -> usize {
        self.state.lock().unwrap().rooms.len()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(rooms: &Rooms, id: ConnId) -> mpsc::Receiver<String> {
        let (tx, rx) = mpsc::channel(16);
        rooms.add(id, tx);
        rx
    }

    #[test]
    fn broadcast_goes_to_members_except_the_sender() {
        let rooms = Rooms::default();
        let mut a = client(&rooms, 1);
        let mut b = client(&rooms, 2);
        let mut c = client(&rooms, 3);
        let mut outsider = client(&rooms, 4);
        for id in [1, 2, 3] {
            assert!(rooms.join(id, "кухня"), "первый вход в комнату должен вернуть true");
        }
        assert_eq!(rooms.broadcast("кухня", 1, "кто за чай?"), 2, "доставлено должно быть двоим: всем, кроме отправителя");
        assert_eq!(b.try_recv().as_deref(), Ok("кто за чай?"));
        assert_eq!(c.try_recv().as_deref(), Ok("кто за чай?"));
        assert!(a.try_recv().is_err(), "отправитель получил своё же сообщение");
        assert!(outsider.try_recv().is_err(), "сообщение ушло тому, кто не в комнате");
    }

    #[test]
    fn rooms_are_independent_and_a_conn_can_be_in_several() {
        let rooms = Rooms::default();
        let mut a = client(&rooms, 1);
        let mut b = client(&rooms, 2);
        rooms.join(1, "кухня");
        rooms.join(1, "балкон");
        rooms.join(2, "балкон");
        assert_eq!(rooms.broadcast("кухня", 1, "на кухне"), 0, "в комнате «кухня» кроме отправителя никого");
        assert_eq!(rooms.broadcast("балкон", 2, "на балконе"), 1);
        assert_eq!(a.try_recv().as_deref(), Ok("на балконе"));
        assert!(b.try_recv().is_err());
    }

    #[test]
    fn join_twice_is_idempotent_and_members_are_sorted() {
        let rooms = Rooms::default();
        let _x = client(&rooms, 5);
        let _y = client(&rooms, 2);
        let _z = client(&rooms, 9);
        assert!(rooms.join(9, "r"));
        assert!(rooms.join(2, "r"));
        assert!(rooms.join(5, "r"));
        assert!(!rooms.join(5, "r"), "повторный вход ничего не меняет и возвращает false");
        assert_eq!(rooms.members("r"), vec![2, 5, 9], "участники отдаются по возрастанию id");
        assert!(!rooms.join(77, "r"), "неизвестное соединение в комнату не пускаем");
    }

    #[test]
    fn writing_to_a_room_you_are_not_in_is_refused() {
        let rooms = Rooms::default();
        let mut inside = client(&rooms, 1);
        let _outside = client(&rooms, 2);
        rooms.join(1, "закрытая");
        assert_eq!(rooms.broadcast("закрытая", 2, "я тут не был"), 0, "посторонний не может писать в комнату");
        assert!(inside.try_recv().is_err());
        assert_eq!(rooms.broadcast("нет такой", 1, "эй"), 0);
    }

    #[test]
    fn empty_rooms_disappear() {
        let rooms = Rooms::default();
        let _a = client(&rooms, 1);
        let _b = client(&rooms, 2);
        rooms.join(1, "один");
        rooms.join(2, "один");
        rooms.join(1, "два");
        assert_eq!(rooms.room_count(), 2);
        assert!(rooms.leave(1, "два"));
        assert_eq!(rooms.room_count(), 1, "комната без участников должна исчезнуть");
        assert!(!rooms.leave(1, "два"), "выйти из комнаты, где тебя нет, нельзя");
        rooms.remove(1);
        rooms.remove(2);
        assert_eq!(rooms.room_count(), 0, "после ухода всех соединений комнат не остаётся");
        assert!(rooms.members("один").is_empty());
    }

    #[test]
    fn removed_connection_leaves_every_room_and_gets_nothing() {
        let rooms = Rooms::default();
        let mut gone = client(&rooms, 1);
        let mut stay = client(&rooms, 2);
        for room in ["a", "b", "c"] {
            rooms.join(1, room);
            rooms.join(2, room);
        }
        rooms.remove(1);
        for room in ["a", "b", "c"] {
            assert_eq!(rooms.members(room), vec![2], "ушедшее соединение осталось в комнате {room}");
        }
        assert_eq!(rooms.broadcast("a", 2, "тихо"), 0);
        assert!(gone.try_recv().is_err());
        assert!(stay.try_recv().is_err());
    }
}
