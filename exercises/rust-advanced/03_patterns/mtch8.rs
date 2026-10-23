// Сессия соединения хранит состояние в enum и умеет с ним работать. Автор написал методы так, как
// естественно думается («посмотрим, в каком мы состоянии»), и компилятор отказался собирать
// половину из них. Заглушка в виде clone годится не везде: где-то достаточно посмотреть, а где-то
// нужно по-настоящему вынуть значение из состояния. Тесты проверяют и итоговое поведение: счётчик
// должен расти, а закрытая сессия помнить собеседника.
// TODO: Заставь `Session` собраться и пройти тесты. Публичные сигнатуры менять не нужно.

#[derive(Debug, PartialEq)]
pub enum Link {
    Idle,
    Connected { peer: String, sent: u32 },
    Closed { peer: String, sent: u32 },
}

pub struct Session {
    pub link: Link,
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

impl Session {
    pub fn new() -> Session {
        Session { link: Link::Idle }
    }

    pub fn connect(&mut self, peer: &str) -> bool {
        match self.link {
            Link::Idle => {
                self.link = Link::Connected { peer: peer.to_string(), sent: 0 };
                true
            }
            _ => false,
        }
    }

    pub fn describe(&self) -> String {
        match self.link {
            Link::Idle => "not connected".to_string(),
            Link::Connected { peer, sent } => format!("connected to {peer}, {sent} sent"),
            Link::Closed { peer, sent } => format!("closed with {peer}, {sent} sent"),
        }
    }

    pub fn peer(&self) -> Option<&str> {
        match self.link {
            Link::Connected { peer, .. } | Link::Closed { peer, .. } => Some(peer.as_str()),
            Link::Idle => None,
        }
    }

    pub fn send(&mut self) -> bool {
        match self.link {
            Link::Connected { mut sent, .. } => {
                sent += 1;
                true
            }
            _ => false,
        }
    }

    pub fn close(&mut self) -> bool {
        self.link = match self.link {
            Link::Connected { peer, sent } => Link::Closed { peer, sent },
            other => return false,
        };
        true
    }
}

fn main() {
    let mut s = Session::new();
    s.connect("ann");
    println!("{}", s.describe());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connected() -> Session {
        let mut s = Session::new();
        assert!(s.connect("ann"), "из Idle подключиться можно");
        s
    }

    #[test]
    fn describing_every_state() {
        let mut s = Session::new();
        assert_eq!(s.describe(), "not connected");
        s.connect("ann");
        assert_eq!(s.describe(), "connected to ann, 0 sent");
        s.send();
        assert_eq!(s.describe(), "connected to ann, 1 sent");
        s.close();
        assert_eq!(s.describe(), "closed with ann, 1 sent");
    }

    #[test]
    fn describing_does_not_consume_the_state() {
        let s = connected();
        let _ = s.describe();
        let _ = s.describe();
        assert_eq!(s.link, Link::Connected { peer: "ann".into(), sent: 0 });
    }

    #[test]
    fn sending_really_counts() {
        let mut s = connected();
        assert!(s.send());
        assert!(s.send());
        assert!(s.send());
        assert_eq!(s.link, Link::Connected { peer: "ann".into(), sent: 3 }, "счётчик в самом состоянии должен вырасти до 3");
        let mut idle = Session::new();
        assert!(!idle.send(), "из Idle отправлять нечего");
    }

    #[test]
    fn peer_is_a_borrow() {
        let s = connected();
        let p = s.peer().expect("собеседник есть");
        assert_eq!(p, "ann");
        let Link::Connected { peer, .. } = &s.link else { panic!("состояние должно быть Connected") };
        assert_eq!(p.as_ptr(), peer.as_ptr(), "peer() отдаёт ссылку на строку в состоянии, а не копию");
        assert_eq!(Session::new().peer(), None);
    }

    #[test]
    fn closing_moves_the_peer_over() {
        let mut s = connected();
        s.send();
        s.send();
        assert!(s.close());
        assert_eq!(s.link, Link::Closed { peer: "ann".into(), sent: 2 }, "закрытая сессия помнит собеседника и счётчик");
        assert_eq!(s.peer(), Some("ann"));
    }

    #[test]
    fn repeated_calls_change_nothing() {
        let mut s = connected();
        assert!(s.close());
        assert!(!s.close(), "второй close ничего не делает");
        assert!(!s.send(), "в закрытую сессию не отправляют");
        assert!(!s.connect("bob"), "закрытая сессия не переподключается");
        assert_eq!(s.link, Link::Closed { peer: "ann".into(), sent: 0 });
        let mut idle = Session::new();
        assert!(!idle.close(), "закрыть неподключённую нельзя");
        assert_eq!(idle.link, Link::Idle, "и состояние при этом не должно испортиться");
        let mut c = connected();
        assert!(!c.connect("bob"), "уже подключённая не подключается снова");
        assert_eq!(c.peer(), Some("ann"));
    }
}
