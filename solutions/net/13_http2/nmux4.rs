// В одном HTTP/2-соединении живёт много вызовов сразу, и у каждого свой номер потока. Клиентские номера нечётные и только
// растут, а кадры разных потоков идут вперемешку. Сервер хранит состояние каждого потока и обязан замечать нарушения:
// чётный номер от клиента, номер из прошлого, данные после конца запроса, данные в потоке, которого не открывали.
// Нарушения бывают двух видов: ломающие только один поток и ломающие всё соединение. TODO: Допиши Streams::on_frame, finish и route.

#![allow(dead_code)]
use std::collections::{BTreeMap, HashMap};

const PROTOCOL_ERROR: u32 = 1;
const STREAM_CLOSED: u32 = 5;
const REFUSED_STREAM: u32 = 7;

#[derive(Debug, PartialEq, Clone)]
enum Frame {
    Headers { stream: u32, end_stream: bool },
    Data { stream: u32, bytes: Vec<u8>, end_stream: bool },
    RstStream { stream: u32, code: u32 },
}

#[derive(Debug, PartialEq)]
enum Violation {
    Connection(u32),
    Stream(u32, u32),
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum State {
    Open,
    HalfClosedRemote,
    Closed,
}

#[derive(Debug, PartialEq, Default)]
struct StreamLog {
    headers: usize,
    body: Vec<u8>,
    ended: bool,
    reset: Option<u32>,
}

struct Streams {
    max_concurrent: usize,
    last_id: u32,
    states: HashMap<u32, State>,
}

impl Streams {
    fn new(max_concurrent: usize) -> Streams {
        Streams { max_concurrent, last_id: 0, states: HashMap::new() }
    }

    fn active(&self) -> usize {
        self.states.values().filter(|s| **s != State::Closed).count()
    }

    fn state(&self, stream: u32) -> Option<State> {
        self.states.get(&stream).copied()
    }

    fn on_frame(&mut self, frame: &Frame) -> Result<(), Violation> {
        match frame {
            Frame::Headers { stream, end_stream } => {
                let stream = *stream;
                match self.states.get(&stream).copied() {
                    None => {
                        if stream % 2 == 0 || stream <= self.last_id {
                            return Err(Violation::Connection(PROTOCOL_ERROR));
                        }
                        self.last_id = stream;
                        if self.active() >= self.max_concurrent {
                            self.states.insert(stream, State::Closed);
                            return Err(Violation::Stream(stream, REFUSED_STREAM));
                        }
                        self.states.insert(stream, if *end_stream { State::HalfClosedRemote } else { State::Open });
                        Ok(())
                    }
                    Some(State::Open) => {
                        if !*end_stream {
                            return Err(Violation::Stream(stream, PROTOCOL_ERROR));
                        }
                        self.states.insert(stream, State::HalfClosedRemote);
                        Ok(())
                    }
                    Some(State::HalfClosedRemote | State::Closed) => Err(Violation::Stream(stream, STREAM_CLOSED)),
                }
            }
            Frame::Data { stream, end_stream, .. } => match self.states.get(stream).copied() {
                None => Err(Violation::Connection(PROTOCOL_ERROR)),
                Some(State::Open) => {
                    if *end_stream {
                        self.states.insert(*stream, State::HalfClosedRemote);
                    }
                    Ok(())
                }
                Some(State::HalfClosedRemote | State::Closed) => Err(Violation::Stream(*stream, STREAM_CLOSED)),
            },
            Frame::RstStream { stream, .. } => match self.states.get(stream).copied() {
                None => Err(Violation::Connection(PROTOCOL_ERROR)),
                Some(_) => {
                    self.states.insert(*stream, State::Closed);
                    Ok(())
                }
            },
        }
    }

    fn finish(&mut self, stream: u32) {
        if let Some(state) = self.states.get_mut(&stream) {
            *state = State::Closed;
        }
    }
}

fn route(streams: &mut Streams, frames: &[Frame]) -> Result<BTreeMap<u32, StreamLog>, Violation> {
    let mut logs: BTreeMap<u32, StreamLog> = BTreeMap::new();
    for frame in frames {
        match streams.on_frame(frame) {
            Ok(()) => {}
            Err(Violation::Stream(stream, code)) => {
                logs.entry(stream).or_default().reset = Some(code);
                continue;
            }
            Err(connection) => return Err(connection),
        }
        match frame {
            Frame::Headers { stream, end_stream } => {
                let log = logs.entry(*stream).or_default();
                log.headers += 1;
                log.ended |= *end_stream;
            }
            Frame::Data { stream, bytes, end_stream } => {
                let log = logs.entry(*stream).or_default();
                log.body.extend_from_slice(bytes);
                log.ended |= *end_stream;
            }
            Frame::RstStream { stream, code } => {
                logs.entry(*stream).or_default().reset = Some(*code);
            }
        }
    }
    Ok(logs)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(stream: u32, end: bool) -> Frame {
        Frame::Headers { stream, end_stream: end }
    }

    fn data(stream: u32, bytes: &str, end: bool) -> Frame {
        Frame::Data { stream, bytes: bytes.as_bytes().to_vec(), end_stream: end }
    }

    #[test]
    fn interleaved_streams_are_separated() {
        let mut streams = Streams::new(100);
        let frames = vec![
            headers(1, false),
            headers(3, false),
            data(1, "he", false),
            headers(5, true),
            data(3, "wor", false),
            data(1, "llo", true),
            data(3, "ld", true),
        ];
        let logs = route(&mut streams, &frames).unwrap();
        assert_eq!(logs.len(), 3);
        assert_eq!(logs[&1], StreamLog { headers: 1, body: b"hello".to_vec(), ended: true, reset: None });
        assert_eq!(logs[&3], StreamLog { headers: 1, body: b"world".to_vec(), ended: true, reset: None });
        assert_eq!(logs[&5], StreamLog { headers: 1, body: vec![], ended: true, reset: None }, "запрос без тела: заголовки с концом потока");
    }

    #[test]
    fn stream_ids_must_be_odd_and_increasing() {
        let mut streams = Streams::new(100);
        assert_eq!(streams.on_frame(&headers(2, false)), Err(Violation::Connection(PROTOCOL_ERROR)), "чётные номера принадлежат серверу");
        assert_eq!(streams.on_frame(&headers(0, false)), Err(Violation::Connection(PROTOCOL_ERROR)));
        let mut streams = Streams::new(100);
        streams.on_frame(&headers(5, false)).unwrap();
        assert_eq!(streams.on_frame(&headers(3, false)), Err(Violation::Connection(PROTOCOL_ERROR)), "номер 3 меньше уже виденного 5: поток закрыт неявно");
        assert!(streams.on_frame(&headers(9, false)).is_ok(), "номера могут идти с пропусками");
    }

    #[test]
    fn data_after_end_stream_is_a_stream_error() {
        let mut streams = Streams::new(100);
        streams.on_frame(&headers(1, false)).unwrap();
        streams.on_frame(&data(1, "x", true)).unwrap();
        assert_eq!(streams.state(1), Some(State::HalfClosedRemote));
        assert_eq!(streams.on_frame(&data(1, "y", false)), Err(Violation::Stream(1, STREAM_CLOSED)), "клиент закончил запрос и не имеет права слать ещё");
        let mut streams = Streams::new(100);
        streams.on_frame(&headers(1, true)).unwrap();
        assert_eq!(streams.on_frame(&data(1, "y", false)), Err(Violation::Stream(1, STREAM_CLOSED)));
    }

    #[test]
    fn data_on_a_stream_that_was_never_opened_is_a_connection_error() {
        let mut streams = Streams::new(100);
        assert_eq!(streams.on_frame(&data(7, "x", false)), Err(Violation::Connection(PROTOCOL_ERROR)));
        assert_eq!(streams.on_frame(&Frame::RstStream { stream: 7, code: 8 }), Err(Violation::Connection(PROTOCOL_ERROR)));
    }

    #[test]
    fn concurrency_limit_refuses_the_stream_but_not_the_connection() {
        let mut streams = Streams::new(2);
        streams.on_frame(&headers(1, false)).unwrap();
        streams.on_frame(&headers(3, false)).unwrap();
        assert_eq!(streams.on_frame(&headers(5, false)), Err(Violation::Stream(5, REFUSED_STREAM)), "третий одновременный вызов сверх лимита");
        assert_eq!(streams.active(), 2);
        streams.finish(1);
        assert_eq!(streams.active(), 1);
        assert!(streams.on_frame(&headers(7, false)).is_ok(), "после завершения первого место освободилось");
    }

    #[test]
    fn reset_closes_the_stream_and_frees_the_slot() {
        let mut streams = Streams::new(1);
        streams.on_frame(&headers(1, false)).unwrap();
        streams.on_frame(&Frame::RstStream { stream: 1, code: 8 }).unwrap();
        assert_eq!(streams.state(1), Some(State::Closed));
        assert!(streams.on_frame(&headers(3, false)).is_ok(), "оборванный вызов освободил место");
        assert_eq!(streams.on_frame(&data(1, "late", false)), Err(Violation::Stream(1, STREAM_CLOSED)), "данные для закрытого потока");
    }

    #[test]
    fn trailers_must_end_the_stream() {
        let mut streams = Streams::new(10);
        streams.on_frame(&headers(1, false)).unwrap();
        streams.on_frame(&data(1, "body", false)).unwrap();
        assert_eq!(streams.on_frame(&headers(1, false)), Err(Violation::Stream(1, PROTOCOL_ERROR)), "второй блок заголовков (трейлеры) обязан нести конец потока");
        let mut streams = Streams::new(10);
        streams.on_frame(&headers(1, false)).unwrap();
        assert!(streams.on_frame(&headers(1, true)).is_ok());
    }

    #[test]
    fn route_records_stream_errors_and_stops_on_connection_errors() {
        let mut streams = Streams::new(10);
        let frames = vec![headers(1, true), data(1, "late", false), headers(3, false), data(3, "ok", true)];
        let logs = route(&mut streams, &frames).unwrap();
        assert_eq!(logs[&1].reset, Some(STREAM_CLOSED), "ошибка одного потока записана в его журнал, остальные продолжают жить");
        assert_eq!(logs[&3].body, b"ok".to_vec());
        let mut streams = Streams::new(10);
        assert_eq!(route(&mut streams, &[headers(1, false), headers(4, false)]), Err(Violation::Connection(PROTOCOL_ERROR)));
    }
}
