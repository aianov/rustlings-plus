use std::time::Duration;

use h2dump::{Call, DumpError, Goaway, GrpcStatus, analyze};

const PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

fn frame(kind: u8, flags: u8, stream: u32, body: &[u8]) -> Vec<u8> {
    let len = body.len() as u32;
    let mut out = vec![(len >> 16) as u8, (len >> 8) as u8, len as u8, kind, flags];
    out.extend_from_slice(&stream.to_be_bytes());
    out.extend_from_slice(body);
    out
}

fn settings(entries: &[(u16, u32)]) -> Vec<u8> {
    let mut body = Vec::new();
    for (id, value) in entries {
        body.extend_from_slice(&id.to_be_bytes());
        body.extend_from_slice(&value.to_be_bytes());
    }
    frame(4, 0, 0, &body)
}

fn settings_ack() -> Vec<u8> {
    frame(4, 1, 0, &[])
}

fn headers(stream: u32, block: &[u8], end_stream: bool) -> Vec<u8> {
    frame(1, 0x4 | u8::from(end_stream), stream, block)
}

fn data(stream: u32, body: &[u8], end_stream: bool) -> Vec<u8> {
    frame(0, u8::from(end_stream), stream, body)
}

fn rst(stream: u32, code: u32) -> Vec<u8> {
    frame(3, 0, stream, &code.to_be_bytes())
}

fn ping(ack: bool) -> Vec<u8> {
    frame(6, u8::from(ack), 0, &[1, 2, 3, 4, 5, 6, 7, 8])
}

fn goaway(last: u32, code: u32, debug: &str) -> Vec<u8> {
    let mut body = last.to_be_bytes().to_vec();
    body.extend_from_slice(&code.to_be_bytes());
    body.extend_from_slice(debug.as_bytes());
    frame(7, 0, 0, &body)
}

fn message(payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0];
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

fn string(s: &str) -> Vec<u8> {
    assert!(s.len() < 127);
    let mut out = vec![s.len() as u8];
    out.extend_from_slice(s.as_bytes());
    out
}

#[derive(Default)]
struct Hpack {
    table: Vec<(String, String)>,
}

impl Hpack {
    fn encode(&mut self, headers: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        for &(name, value) in headers {
            let exact_static = match (name, value) {
                (":method", "POST") => Some(3),
                (":scheme", "http") => Some(6),
                (":status", "200") => Some(8),
                _ => None,
            };
            if let Some(index) = exact_static {
                out.push(0x80 | index);
                continue;
            }
            if let Some(pos) = self.table.iter().position(|(n, v)| n == name && v == value) {
                out.push(0x80 | (62 + pos as u8));
                continue;
            }
            let name_index = match name {
                ":authority" => Some(1),
                ":path" => Some(4),
                "content-type" => Some(31),
                _ => self.table.iter().position(|(n, _)| n == name).map(|p| 62 + p as u8),
            };
            match name_index {
                Some(index) => out.push(0x40 | index),
                None => {
                    out.push(0x40);
                    out.extend(string(name));
                }
            }
            out.extend(string(value));
            self.table.insert(0, (name.to_string(), value.to_string()));
        }
        out
    }
}

fn request_headers(hpack: &mut Hpack, path: &str, timeout: Option<&str>) -> Vec<u8> {
    let mut list = vec![(":method", "POST"), (":scheme", "http"), (":path", path), (":authority", "svc.test"), ("content-type", "application/grpc"), ("te", "trailers")];
    if let Some(t) = timeout {
        list.push(("grpc-timeout", t));
    }
    hpack.encode(&list)
}

fn response_headers(hpack: &mut Hpack) -> Vec<u8> {
    hpack.encode(&[(":status", "200"), ("content-type", "application/grpc")])
}

fn trailers(hpack: &mut Hpack, status: &str, message: Option<&str>) -> Vec<u8> {
    let mut list = vec![("grpc-status", status)];
    if let Some(m) = message {
        list.push(("grpc-message", m));
    }
    hpack.encode(&list)
}

fn concat(parts: Vec<Vec<u8>>) -> Vec<u8> {
    parts.into_iter().flatten().collect()
}

fn client(parts: Vec<Vec<u8>>) -> Vec<u8> {
    let mut out = PREFACE.to_vec();
    out.extend(concat(parts));
    out
}

fn call_of(report: &h2dump::Report, stream: u32) -> &Call {
    report.calls.iter().find(|c| c.stream == stream).unwrap_or_else(|| panic!("в отчёте нет вызова на потоке {stream}: {:?}", report.calls.iter().map(|c| c.stream).collect::<Vec<_>>()))
}

#[test]
fn simple_unary_call() {
    let mut c = Hpack::default();
    let mut s = Hpack::default();
    let client_bytes = client(vec![settings(&[]), headers(1, &request_headers(&mut c, "/echo.Echo/Say", Some("100m")), false), data(1, &message(b"ping"), true)]);
    let server_bytes = concat(vec![settings(&[]), settings_ack(), headers(1, &response_headers(&mut s), false), data(1, &message(b"pong"), false), headers(1, &trailers(&mut s, "0", None), true)]);
    let report = analyze(&client_bytes, &server_bytes).unwrap();
    assert_eq!(report.calls.len(), 1);
    assert_eq!(
        report.calls[0],
        Call {
            stream: 1,
            path: "/echo.Echo/Say".to_string(),
            timeout: Some(Duration::from_millis(100)),
            request_messages: vec![b"ping".to_vec()],
            response_messages: vec![b"pong".to_vec()],
            status: Some(GrpcStatus { code: 0, message: String::new() }),
            reset: None,
        }
    );
}

#[test]
fn interleaved_calls_with_dynamic_table_and_split_messages() {
    let mut c = Hpack::default();
    let mut s = Hpack::default();
    let first = message(b"one-one");
    let two = [message(b"a"), message(b"bb")].concat();
    let h1 = request_headers(&mut c, "/pkg.Svc/One", None);
    let h3 = request_headers(&mut c, "/pkg.Svc/Two", Some("2S"));
    assert!(h3.len() < h1.len(), "проверка самого теста: второй блок обязан ссылаться на динамическую таблицу и быть короче");
    let client_bytes = client(vec![
        settings(&[(3, 100)]),
        headers(1, &h1, false),
        headers(3, &h3, false),
        data(1, &first[..4], false),
        data(3, &two, false),
        data(1, &first[4..], true),
        data(3, &[], true),
    ]);
    let r3h = response_headers(&mut s);
    let r3t = trailers(&mut s, "0", None);
    let r1h = response_headers(&mut s);
    let r1t = trailers(&mut s, "0", None);
    let r1 = [message(b"r1a"), message(b"r1b")].concat();
    let server_bytes = concat(vec![
        settings(&[]),
        headers(3, &r3h, false),
        data(3, &message(b"re-two"), false),
        headers(3, &r3t, true),
        headers(1, &r1h, false),
        data(1, &r1, false),
        headers(1, &r1t, true),
    ]);
    let report = analyze(&client_bytes, &server_bytes).unwrap();
    assert_eq!(report.calls.iter().map(|c| c.stream).collect::<Vec<_>>(), vec![1, 3], "вызовы идут по возрастанию номера потока");
    let one = call_of(&report, 1);
    assert_eq!((one.path.as_str(), one.timeout), ("/pkg.Svc/One", None));
    assert_eq!(one.request_messages, vec![b"one-one".to_vec()], "сообщение разрезано между двумя DATA");
    assert_eq!(one.response_messages, vec![b"r1a".to_vec(), b"r1b".to_vec()], "два сообщения в одном DATA");
    let two = call_of(&report, 3);
    assert_eq!((two.path.as_str(), two.timeout), ("/pkg.Svc/Two", Some(Duration::from_secs(2))));
    assert_eq!(two.request_messages, vec![b"a".to_vec(), b"bb".to_vec()]);
    assert_eq!(two.response_messages, vec![b"re-two".to_vec()]);
    assert_eq!(two.status, Some(GrpcStatus { code: 0, message: String::new() }));
    assert_eq!(report.client_settings, vec![(3, 100)]);
}

#[test]
fn trailers_only_error_with_percent_encoded_message() {
    let mut c = Hpack::default();
    let mut s = Hpack::default();
    let client_bytes = client(vec![headers(5, &request_headers(&mut c, "/users.Users/Get", Some("250m")), false), data(5, &message(b"who?"), true)]);
    let block = s.encode(&[(":status", "200"), ("content-type", "application/grpc"), ("grpc-status", "5"), ("grpc-message", "no%20such%20user%20%D0%AF")]);
    let server_bytes = concat(vec![headers(5, &block, true)]);
    let report = analyze(&client_bytes, &server_bytes).unwrap();
    let call = call_of(&report, 5);
    assert_eq!(call.status, Some(GrpcStatus { code: 5, message: "no such user Я".to_string() }), "сообщение из заголовка нужно раскодировать из процентной записи");
    assert!(call.response_messages.is_empty());
    assert_eq!(call.timeout, Some(Duration::from_millis(250)));
}

#[test]
fn resets_from_both_sides() {
    let mut c = Hpack::default();
    let client_bytes = client(vec![
        headers(7, &request_headers(&mut c, "/slow.Slow/Wait", Some("5S")), false),
        headers(9, &request_headers(&mut c, "/slow.Slow/Other", None), true),
        rst(7, 8),
    ]);
    let server_bytes = concat(vec![rst(9, 7)]);
    let report = analyze(&client_bytes, &server_bytes).unwrap();
    let wait = call_of(&report, 7);
    assert_eq!((wait.reset, wait.status.clone()), (Some(8), None), "клиент оборвал вызов (CANCEL), итога от сервера нет");
    assert_eq!(wait.timeout, Some(Duration::from_secs(5)));
    assert_eq!(call_of(&report, 9).reset, Some(7), "сервер отказал в вызове (REFUSED_STREAM)");
}

#[test]
fn settings_pings_and_goaway() {
    let mut c = Hpack::default();
    let client_bytes = client(vec![
        settings(&[(3, 100), (4, 1_048_576)]),
        settings_ack(),
        ping(false),
        ping(false),
        ping(true),
        headers(1, &request_headers(&mut c, "/a.A/B", None), true),
    ]);
    let server_bytes = concat(vec![settings(&[(5, 32_768)]), settings_ack(), ping(true), ping(false), goaway(3, 0, "bye")]);
    let report = analyze(&client_bytes, &server_bytes).unwrap();
    assert_eq!(report.client_settings, vec![(3, 100), (4, 1_048_576)], "подтверждения SETTINGS настройками не считаются");
    assert_eq!(report.server_settings, vec![(5, 32_768)]);
    assert_eq!(report.pings, 3, "считаются PING без флага ACK, из обоих направлений");
    assert_eq!(report.goaway, Some(Goaway { last_stream: 3, code: 0, debug: "bye".to_string() }));
}

#[test]
fn padding_priority_and_unknown_frames() {
    let mut c = Hpack::default();
    let mut s = Hpack::default();
    let block = request_headers(&mut c, "/pad.Pad/Do", None);
    let mut body = vec![2, 0, 0, 0, 0, 15];
    body.extend_from_slice(&block);
    body.extend_from_slice(&[0, 0]);
    let padded_headers = frame(1, 0x4 | 0x8 | 0x20, 1, &body);
    let msg = message(b"padded body");
    let mut padded_data_body = vec![3];
    padded_data_body.extend_from_slice(&msg);
    padded_data_body.extend_from_slice(&[0, 0, 0]);
    let client_bytes = client(vec![frame(0xfa, 0xff, 0, b"extension"), padded_headers, frame(0, 0x1 | 0x8, 1, &padded_data_body)]);
    let server_bytes = concat(vec![frame(0xfb, 0, 3, b"x"), headers(1, &response_headers(&mut s), false), headers(1, &trailers(&mut s, "0", None), true)]);
    let report = analyze(&client_bytes, &server_bytes).unwrap();
    let call = call_of(&report, 1);
    assert_eq!(call.path, "/pad.Pad/Do");
    assert_eq!(call.request_messages, vec![b"padded body".to_vec()], "выравнивание снято, лишние нули не попали в сообщение");
    assert_eq!(call.status.as_ref().map(|s| s.code), Some(0));
}

#[test]
fn hpack_state_is_per_connection_direction() {
    let mut c = Hpack::default();
    let mut s = Hpack::default();
    let h1 = request_headers(&mut c, "/a.A/One", None);
    let h2 = request_headers(&mut c, "/a.A/Two", None);
    let client_bytes = client(vec![headers(1, &h1, true), headers(3, &h2, true)]);
    let server_bytes = concat(vec![headers(1, &trailers(&mut s, "0", None), true), headers(3, &trailers(&mut s, "13", Some("boom")), true)]);
    let report = analyze(&client_bytes, &server_bytes).unwrap();
    assert_eq!(call_of(&report, 3).path, "/a.A/Two");
    assert_eq!(call_of(&report, 3).status, Some(GrpcStatus { code: 13, message: "boom".to_string() }));
    assert_eq!(call_of(&report, 1).status, Some(GrpcStatus { code: 0, message: String::new() }));
}

#[test]
fn broken_dumps_are_errors_not_panics() {
    let mut c = Hpack::default();
    let good_headers = headers(1, &request_headers(&mut c, "/a.A/B", None), false);
    assert_eq!(analyze(b"GET / HTTP/1.1\r\n\r\n", &[]), Err(DumpError::BadPreface));
    assert_eq!(analyze(b"PRI * HTTP/2.0", &[]), Err(DumpError::BadPreface), "прелюдия оборвана");

    let mut truncated = client(vec![good_headers.clone(), data(1, &message(b"xyz"), true)]);
    truncated.pop();
    assert_eq!(analyze(&truncated, &[]), Err(DumpError::Incomplete), "дамп оборвался посреди кадра");

    let bad_index = client(vec![headers(1, &[0xff, 0x7f], false)]);
    assert!(matches!(analyze(&bad_index, &[]), Err(DumpError::Hpack(_))), "ссылка на несуществующую запись таблицы");

    let compressed = client(vec![good_headers.clone(), data(1, &[1, 0, 0, 0, 1, 9], true)]);
    assert!(matches!(analyze(&compressed, &[]), Err(DumpError::Grpc(_))), "сжатые сообщения не поддерживаются");

    let continued = client(vec![frame(1, 0x0, 1, &[0x82])]);
    assert!(matches!(analyze(&continued, &[]), Err(DumpError::Unsupported(_))), "блок заголовков, разбитый на CONTINUATION, честно не поддерживается");

    let bad_ping = client(vec![frame(6, 0, 0, &[1, 2, 3])]);
    assert!(matches!(analyze(&bad_ping, &[]), Err(DumpError::Frame(_))));
}

#[test]
fn garbage_after_the_preface_never_panics() {
    let mut x: u64 = 17;
    for _ in 0..500 {
        let mut dump = PREFACE.to_vec();
        for _ in 0..(x % 60) {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            dump.push((x >> 56) as u8);
        }
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let _ = analyze(&dump, &dump[PREFACE.len()..]);
    }
}
