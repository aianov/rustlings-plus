// Пользователь загружает «аватар» `me.png`, а внутри HTML со скриптом, и сервис отдаёт его с тем `Content-Type`, который
// прислал клиент (`text/html`). Любой, кто откроет ссылку, выполняет чужой скрипт на нашем домене с нашими cookie. Вторая
// дыра: имя файла с кавычкой и переводом строки попадает в `Content-Disposition` как есть и позволяет дописать в ответ
// свои заголовки. Цель: `serve` сам решает, чем является файл (по сигнатуре, как в прошлых задачах). Картинки PNG, JPEG,
// WebP отдаёт с их настоящим типом и `inline`, всё остальное как `application/octet-stream` и `attachment`. Всегда ставит
// `X-Content-Type-Options: nosniff` и `Content-Security-Policy: default-src 'none'; sandbox`. Имя в `Content-Disposition`
// безопасно: ASCII-вариант `filename="..."` и `filename*=UTF-8''...` с процентным кодированием.

fn main() {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Png,
    Jpeg,
    WebP,
}

impl Kind {
    fn mime(self) -> &'static str {
        match self {
            Kind::Png => "image/png",
            Kind::Jpeg => "image/jpeg",
            Kind::WebP => "image/webp",
        }
    }
}

fn sniff(head: &[u8]) -> Option<Kind> {
    if head.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some(Kind::Png)
    } else if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(Kind::Jpeg)
    } else if head.len() >= 12 && head[..4] == b"RIFF"[..] && head[8..12] == b"WEBP"[..] {
        Some(Kind::WebP)
    } else {
        None
    }
}

pub struct Meta {
    pub client_content_type: String,
    pub client_name: String,
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

pub fn serve(meta: &Meta, content: &[u8]) -> Response {
    let _ = sniff(content).map(Kind::mime);
    Response {
        status: 200,
        headers: vec![
            ("Content-Type".into(), meta.client_content_type.clone()),
            ("Content-Disposition".into(), format!("inline; filename=\"{}\"", meta.client_name)),
            ("Content-Length".into(), content.len().to_string()),
        ],
        body: content.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(content_type: &str, name: &str) -> Meta {
        Meta { client_content_type: content_type.into(), client_name: name.into() }
    }

    fn png() -> Vec<u8> {
        let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        v.extend_from_slice(&[0; 20]);
        v
    }

    #[test]
    fn html_pretending_to_be_an_image_is_served_as_a_download() {
        let page = b"<html><script>fetch('/steal')</script></html>";
        let response = serve(&meta("text/html", "me.png"), page);
        assert_eq!(response.header("Content-Type"), Some("application/octet-stream"), "тип выбирает сервер, а не клиент: HTML отдаётся как набор байт");
        assert!(response.header("Content-Disposition").unwrap().starts_with("attachment"), "и как скачивание, а не показ страницы: {:?}", response.header("Content-Disposition"));
        assert_eq!(response.header("X-Content-Type-Options"), Some("nosniff"), "браузеру запрещено угадывать тип по содержимому");
        assert_eq!(response.body, page, "тело не меняется");
    }

    #[test]
    fn svg_with_a_script_is_also_a_download() {
        let svg = b"<svg xmlns='http://www.w3.org/2000/svg' onload='alert(1)'></svg>";
        let response = serve(&meta("image/svg+xml", "logo.svg"), svg);
        assert_eq!(response.header("Content-Type"), Some("application/octet-stream"), "SVG это документ со скриптами, а не картинка из нашего списка");
        assert!(response.header("Content-Disposition").unwrap().starts_with("attachment"), "SVG отдаётся как скачивание");
    }

    #[test]
    fn a_real_image_is_shown_with_its_own_type_whatever_the_client_said() {
        let response = serve(&meta("text/html", "page.html"), &png());
        assert_eq!(response.header("Content-Type"), Some("image/png"), "настоящий PNG отдаётся как image/png, хоть клиент и заявил text/html");
        assert!(response.header("Content-Disposition").unwrap().starts_with("inline"), "картинку показываем в браузере");
        assert_eq!(response.header("X-Content-Type-Options"), Some("nosniff"), "и для картинок nosniff тоже");
        assert_eq!(response.header("Content-Security-Policy"), Some("default-src 'none'; sandbox"), "политика, которая запрещает и скрипты, и подгрузки");
        assert_eq!(response.status, 200, "статус 200");
    }

    #[test]
    fn a_hostile_file_name_cannot_inject_headers() {
        let response = serve(&meta("image/png", "x.png\r\nSet-Cookie: session=evil"), &png());
        for (name, value) in &response.headers {
            assert!(!value.contains('\r') && !value.contains('\n'), "в значении заголовка {name} перевод строки: {value:?}");
            assert!(!name.eq_ignore_ascii_case("Set-Cookie"), "появился чужой заголовок Set-Cookie");
        }
    }

    #[test]
    fn a_quote_in_the_name_cannot_break_out_of_the_filename() {
        let response = serve(&meta("image/png", "a\"; filename=\"evil.exe"), &png());
        let disposition = response.header("Content-Disposition").unwrap();
        assert_eq!(disposition.matches('"').count(), 2, "в Content-Disposition ровно одна пара кавычек, вокруг ASCII-имени: {disposition}");
        assert!(!disposition.contains("filename=\"evil"), "кавычка из имени не должна закрывать значение: {disposition}");
    }

    #[test]
    fn non_ascii_names_get_both_forms() {
        let response = serve(&meta("image/png", "фото.png"), &png());
        let disposition = response.header("Content-Disposition").unwrap();
        assert!(disposition.contains("filename=\"____.png\""), "ASCII-вариант заменяет не-ASCII символы подчёркиванием: {disposition}");
        assert!(disposition.contains("filename*=UTF-8''%D1%84%D0%BE%D1%82%D0%BE.png"), "настоящее имя в процентном кодировании UTF-8: {disposition}");
    }

    #[test]
    fn an_empty_or_junk_name_still_gives_a_valid_header() {
        let response = serve(&meta("image/png", "\r\n"), &png());
        let disposition = response.header("Content-Disposition").unwrap();
        assert!(disposition.contains("filename=\"file\""), "когда от имени ничего не осталось, подставляется file: {disposition}");
    }
}
