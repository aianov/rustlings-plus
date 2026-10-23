fn main() {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Png,
    Jpeg,
    WebP,
}

impl Kind {
    pub fn mime(self) -> &'static str {
        match self {
            Kind::Png => "image/png",
            Kind::Jpeg => "image/jpeg",
            Kind::WebP => "image/webp",
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            Kind::Png => "png",
            Kind::Jpeg => "jpg",
            Kind::WebP => "webp",
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct NotAnImage;

pub fn sniff(head: &[u8]) -> Option<Kind> {
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

pub fn accept(file_name: &str, content_type: &str, head: &[u8]) -> Result<Kind, NotAnImage> {
    let _ = (file_name, content_type);
    sniff(head).ok_or(NotAnImage)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        v.extend_from_slice(&[0, 0, 0, 13, b'I', b'H', b'D', b'R']);
        v
    }

    fn jpeg() -> Vec<u8> {
        vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0]
    }

    fn webp() -> Vec<u8> {
        let mut v = b"RIFF".to_vec();
        v.extend_from_slice(&[0x24, 0, 0, 0]);
        v.extend_from_slice(b"WEBPVP8 ");
        v
    }

    #[test]
    fn the_three_formats_are_recognised_by_their_first_bytes() {
        assert_eq!(accept("x", "x", &png()), Ok(Kind::Png), "PNG: восемь байт сигнатуры");
        assert_eq!(accept("x", "x", &jpeg()), Ok(Kind::Jpeg), "JPEG: FF D8 FF");
        assert_eq!(accept("x", "x", &webp()), Ok(Kind::WebP), "WebP: RIFF, четыре байта длины, WEBP");
    }

    #[test]
    fn the_claimed_name_and_type_decide_nothing() {
        assert_eq!(accept("holiday.exe", "application/octet-stream", &png()), Ok(Kind::Png), "настоящий PNG с чужим именем и типом остаётся PNG");
        assert_eq!(accept("photo.jpg", "image/jpeg", &png()), Ok(Kind::Png), "заявили JPEG, а внутри PNG: верим сигнатуре");
        assert_eq!(accept("me.png", "image/png", b"<html><script>alert(1)</script></html>"), Err(NotAnImage), "HTML под именем .png и типом image/png не картинка");
        assert_eq!(accept("me.png", "image/png", b"MZ\x90\x00\x03\x00\x00\x00"), Err(NotAnImage), "исполняемый файл не картинка, что бы ни заявили");
    }

    #[test]
    fn near_misses_are_not_images() {
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&[0x24, 0, 0, 0]);
        wav.extend_from_slice(b"WAVEfmt ");
        assert_eq!(accept("a.webp", "image/webp", &wav), Err(NotAnImage), "RIFF бывает и у звука: решает слово WEBP на смещении 8");
        assert_eq!(accept("a.gif", "image/gif", b"GIF89a\x01\x00\x01\x00"), Err(NotAnImage), "GIF в этот курс не входит");
        assert_eq!(accept("a.jpg", "image/jpeg", &[0xFF, 0xD8, 0x00, 0xE0]), Err(NotAnImage), "FF D8 без третьего FF это не JPEG");
        let mut almost = png();
        almost[4] = 0x00;
        assert_eq!(accept("a.png", "image/png", &almost), Err(NotAnImage), "сигнатура PNG проверяется вся, а не только слово PNG");
    }

    #[test]
    fn short_and_empty_input_is_just_not_an_image() {
        assert_eq!(accept("a.png", "image/png", &[]), Err(NotAnImage), "пустое тело");
        assert_eq!(accept("a.png", "image/png", &png()[..5]), Err(NotAnImage), "оборванная сигнатура PNG");
        assert_eq!(accept("a.webp", "image/webp", &webp()[..10]), Err(NotAnImage), "короче двенадцати байт WebP не опознать, и паниковать нельзя");
        assert_eq!(accept("a.jpg", "image/jpeg", &[0xFF, 0xD8]), Err(NotAnImage), "два байта JPEG ещё не JPEG");
    }

    #[test]
    fn an_accepted_kind_knows_its_own_mime_and_extension() {
        let kind = accept("evil.html", "text/html", &jpeg()).unwrap();
        assert_eq!((kind.mime(), kind.ext()), ("image/jpeg", "jpg"), "тип и расширение берутся у распознанного вида, а не у клиента");
    }
}
