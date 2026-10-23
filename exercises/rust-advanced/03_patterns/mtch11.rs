// Соединение с клиентом живёт по таблице:
//   Connecting + Opened      -> Open
//   Open       + Message     -> счётчик delivered + 1
//   Open       + Ping        -> счётчик pongs + 1
//   Open       + Close       -> Closing
//   Closing    + Message     -> принимается, но не доставляется: счётчик dropped + 1
//   Closing    + CloseAck    -> Closed
// Любое другое событие в любом состоянии это ошибка `BadEvent`, при этом ничего не меняется:
// ни состояние, ни счётчики. Сервер в проде начал вести себя странно: после Closed он «принимает»
// сообщения, а число доставленных у закрывающихся соединений не сходится с логами клиентов.
// Код выглядит рабочим и даже собирается без предупреждений.
// TODO: Найди, где реальное поведение расходится с таблицей, и исправь.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    Connecting,
    Open,
    Closing,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    Opened,
    Message,
    Ping,
    Close,
    CloseAck,
}

#[derive(Debug, PartialEq)]
pub struct BadEvent {
    pub state: State,
    pub event: Event,
}

#[derive(Debug)]
pub struct Conn {
    pub state: State,
    pub delivered: u32,
    pub pongs: u32,
    pub dropped: u32,
}

impl Default for Conn {
    fn default() -> Self {
        Conn::new()
    }
}

impl Conn {
    pub fn new() -> Conn {
        Conn { state: State::Connecting, delivered: 0, pongs: 0, dropped: 0 }
    }

    pub fn handle(&mut self, event: Event) -> Result<(), BadEvent> {
        match (self.state, event) {
            (State::Connecting, Event::Opened) => self.state = State::Open,
            (State::Open | State::Closing, Event::Message) => self.delivered += 1,
            (State::Open | State::Closing, Event::Ping) => self.pongs += 1,
            (State::Open, Event::Close) => self.state = State::Closing,
            (State::Closing, Event::CloseAck) => self.state = State::Closed,
            _ => {}
        }
        Ok(())
    }
}

fn main() {
    let mut c = Conn::new();
    println!("{:?}", c.handle(Event::Opened));
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATES: [State; 4] = [State::Connecting, State::Open, State::Closing, State::Closed];
    const EVENTS: [Event; 5] = [Event::Opened, Event::Message, Event::Ping, Event::Close, Event::CloseAck];

    fn in_state(state: State) -> Conn {
        Conn { state, delivered: 7, pongs: 8, dropped: 9 }
    }

    fn allowed(state: State, event: Event) -> bool {
        matches!(
            (state, event),
            (State::Connecting, Event::Opened)
                | (State::Open, Event::Message | Event::Ping | Event::Close)
                | (State::Closing, Event::Message | Event::CloseAck)
        )
    }

    #[test]
    fn invalid_events_are_errors_and_change_nothing() {
        for s in STATES {
            for e in EVENTS {
                if allowed(s, e) {
                    continue;
                }
                let mut c = in_state(s);
                assert_eq!(c.handle(e), Err(BadEvent { state: s, event: e }), "{e:?} в {s:?} недопустимо: нужен BadEvent с этими состоянием и событием");
                assert_eq!((c.state, c.delivered, c.pongs, c.dropped), (s, 7, 8, 9), "после отказа на {e:?} в {s:?} ничего не должно измениться");
            }
        }
    }

    #[test]
    fn valid_events_are_ok() {
        for s in STATES {
            for e in EVENTS {
                if allowed(s, e) {
                    assert_eq!(in_state(s).handle(e), Ok(()), "{e:?} в {s:?} допустимо");
                }
            }
        }
    }

    #[test]
    fn normal_conversation() {
        let mut c = Conn::new();
        c.handle(Event::Opened).expect("открытие");
        c.handle(Event::Message).expect("сообщение");
        c.handle(Event::Message).expect("сообщение");
        c.handle(Event::Ping).expect("пинг");
        assert_eq!((c.state, c.delivered, c.pongs, c.dropped), (State::Open, 2, 1, 0));
        c.handle(Event::Close).expect("закрытие");
        assert_eq!(c.state, State::Closing);
    }

    #[test]
    fn messages_while_closing_are_dropped_not_delivered() {
        let mut c = Conn::new();
        c.handle(Event::Opened).expect("открытие");
        c.handle(Event::Message).expect("сообщение");
        c.handle(Event::Close).expect("закрытие");
        c.handle(Event::Message).expect("сообщение в пути");
        c.handle(Event::Message).expect("ещё одно");
        assert_eq!((c.delivered, c.dropped), (1, 2), "при закрытии сообщения считаются в dropped, а delivered не растёт");
        c.handle(Event::CloseAck).expect("подтверждение");
        assert_eq!(c.state, State::Closed);
        assert!(c.handle(Event::Message).is_err(), "в закрытое соединение сообщения не принимаются");
    }
}
