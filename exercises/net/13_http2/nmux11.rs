// Ответ gRPC на HTTP/2 имеет форму, которую легко испортить: сначала заголовки (:status 200 и content-type), потом сообщения в DATA,
// а итог вызова приходит в самом конце, в трейлерах (grpc-status и grpc-message). Поэтому статус «ошибка» после десяти успешных
// сообщений возможен, а HTTP-статус при этом остаётся 200. Если сообщений не было вовсе, всё умещается в один блок заголовков
// («trailers only»). Текст ошибки может содержать любые символы, а в заголовок годится только печатный ASCII.
// TODO: Допиши layout, percent_encode, percent_decode и parse_trailers. Префикс сообщения и список частей ответа уже есть.

#![allow(dead_code)]

type Header = (String, String);

#[derive(Debug, PartialEq)]
enum Part {
    Headers { headers: Vec<Header>, end_stream: bool },
    Data(Vec<u8>),
}

struct Response {
    messages: Vec<Vec<u8>>,
    status: u32,
    message: String,
}

fn encode_message(payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0];
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

fn h(name: &str, value: &str) -> Header {
    (name.to_string(), value.to_string())
}

fn percent_encode(text: &str) -> String {
    let _ = text;
    todo!()
}

fn percent_decode(text: &str) -> String {
    let _ = text;
    todo!()
}

fn layout(response: &Response) -> Vec<Part> {
    let _ = response;
    todo!()
}

fn parse_trailers(headers: &[Header]) -> (u32, String) {
    let _ = headers;
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(messages: &[&[u8]], status: u32, message: &str) -> Response {
        Response { messages: messages.iter().map(|m| m.to_vec()).collect(), status, message: message.to_string() }
    }

    #[test]
    fn success_has_headers_data_and_trailers() {
        let parts = layout(&response(&[b"one", b"two"], 0, ""));
        assert_eq!(
            parts,
            vec![
                Part::Headers { headers: vec![h(":status", "200"), h("content-type", "application/grpc")], end_stream: false },
                Part::Data(encode_message(b"one")),
                Part::Data(encode_message(b"two")),
                Part::Headers { headers: vec![h("grpc-status", "0")], end_stream: true },
            ],
            "конец потока ставится только на трейлерах, пустое сообщение grpc-message не пишется"
        );
    }

    #[test]
    fn error_after_messages_keeps_http_200() {
        let parts = layout(&response(&[b"partial"], 13, "сбой посреди потока"));
        assert_eq!(parts.len(), 3);
        match &parts[2] {
            Part::Headers { headers, end_stream } => {
                assert!(*end_stream);
                assert_eq!(headers[0], h("grpc-status", "13"));
                assert_eq!(headers[1], h("grpc-message", &percent_encode("сбой посреди потока")));
                assert!(headers.iter().all(|(k, _)| k != ":status"), "в трейлерах нет :status: он ушёл в первом блоке");
            }
            other => panic!("ждали трейлеры, получили {other:?}"),
        }
    }

    #[test]
    fn no_messages_means_trailers_only() {
        let parts = layout(&response(&[], 5, "no such user"));
        assert_eq!(
            parts,
            vec![Part::Headers {
                headers: vec![h(":status", "200"), h("content-type", "application/grpc"), h("grpc-status", "5"), h("grpc-message", "no such user")],
                end_stream: true,
            }],
            "ошибка без единого сообщения: один блок заголовков с концом потока"
        );
        let ok_empty = layout(&response(&[], 0, ""));
        assert_eq!(ok_empty.len(), 1);
    }

    #[test]
    fn percent_encoding_of_messages() {
        assert_eq!(percent_encode("plain text 123"), "plain text 123", "печатный ASCII остаётся как есть, пробел тоже");
        assert_eq!(percent_encode("100%"), "100%25", "сам знак процента кодируется");
        assert_eq!(percent_encode("a\nb\r\tc"), "a%0Ab%0D%09c");
        assert_eq!(percent_encode("Привет"), "%D0%9F%D1%80%D0%B8%D0%B2%D0%B5%D1%82", "кодируются байты UTF-8, буквы верхнего регистра");
        assert_eq!(percent_encode("~ \u{7f}"), "~ %7F");
    }

    #[test]
    fn percent_decoding_is_lenient() {
        assert_eq!(percent_decode("%D0%9F%D1%80%D0%B8%D0%B2%D0%B5%D1%82"), "Привет");
        assert_eq!(percent_decode("100%25"), "100%");
        assert_eq!(percent_decode("%d0%9f"), "П", "шестнадцатеричные цифры любого регистра");
        assert_eq!(percent_decode("50% off"), "50% off", "знак процента без двух цифр остаётся как есть");
        assert_eq!(percent_decode("%zz%4"), "%zz%4");
        assert_eq!(percent_decode("end%"), "end%");
        assert_eq!(percent_decode("end%4"), "end%4");
        assert_eq!(percent_decode("ok%41"), "okA", "последний трёхсимвольный кусок на краю строки");
    }

    #[test]
    fn roundtrip_of_arbitrary_text() {
        for text in ["", "simple", "с юникодом и 😀", "a%b%%c", "строка\nс переводом\r\n", "\u{0}\u{1}\u{7f}"] {
            assert_eq!(percent_decode(&percent_encode(text)), text);
            assert!(percent_encode(text).bytes().all(|b| (0x20..=0x7e).contains(&b)), "после кодирования только печатный ASCII");
        }
    }

    #[test]
    fn trailers_to_status() {
        assert_eq!(parse_trailers(&[h("grpc-status", "0")]), (0, String::new()));
        assert_eq!(parse_trailers(&[h("grpc-status", "3"), h("grpc-message", "bad%20field")]), (3, "bad field".to_string()));
        assert_eq!(parse_trailers(&[h("grpc-message", "x")]).0, 2, "нет grpc-status: итог неизвестен, это UNKNOWN (2), а не успех");
        assert_eq!(parse_trailers(&[h("grpc-status", "abc")]).0, 2);
        assert_eq!(parse_trailers(&[h("grpc-status", "99")]).0, 2, "несуществующий код читается как UNKNOWN");
        assert_eq!(parse_trailers(&[h("grpc-status", "16")]).0, 16);
        assert_eq!(parse_trailers(&[h("Grpc-Status", "4")]).0, 4, "имена заголовков не зависят от регистра");
    }
}
