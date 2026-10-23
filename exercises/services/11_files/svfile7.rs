// Аватар на килобайт проходит проверку размера файла, но по заголовку это картинка 60000 на 60000 пикселей. Когда
// сервис откроет её, чтобы сделать миниатюру, он выделит под пиксели около 14 ГБ памяти и ляжет: «бомба
// декомпрессии». Размер файла говорит о картинке мало, нужны размеры из заголовка. Цель: `check_avatar` достаёт ширину
// и высоту из заголовка PNG и JPEG, не декодируя картинку, и отвергает сторону больше `MAX_SIDE`, площадь больше
// `MAX_PIXELS` и нулевой размер. Обрубленный заголовок это `Truncated`, WebP пока не принимаем (`Unsupported`).

fn main() {}

pub const MAX_BYTES: usize = 1024 * 1024;
pub const MAX_SIDE: u32 = 8192;
pub const MAX_PIXELS: u64 = 16_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Png,
    Jpeg,
    WebP,
}

#[derive(Debug, PartialEq)]
pub enum AvatarError {
    NotImage,
    TooBig,
    BadSize { width: u32, height: u32 },
    Truncated,
    Unsupported,
}

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

pub fn dimensions(bytes: &[u8], kind: Kind) -> Result<(u32, u32), AvatarError> {
    let _ = (bytes, kind);
    todo!()
}

pub fn check_avatar(bytes: &[u8]) -> Result<Kind, AvatarError> {
    if bytes.len() > MAX_BYTES {
        return Err(AvatarError::TooBig);
    }
    sniff(bytes).ok_or(AvatarError::NotImage)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
        v.extend_from_slice(&width.to_be_bytes());
        v.extend_from_slice(&height.to_be_bytes());
        v.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
        v
    }

    fn jpeg_with(sof: u8, width: u16, height: u16, app_payload: &[u8]) -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0];
        v.extend_from_slice(&((app_payload.len() + 2) as u16).to_be_bytes());
        v.extend_from_slice(app_payload);
        v.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x04, 0x01, 0x02]);
        v.extend_from_slice(&[0xFF, sof, 0x00, 0x11, 0x08]);
        v.extend_from_slice(&height.to_be_bytes());
        v.extend_from_slice(&width.to_be_bytes());
        v.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
        v.extend_from_slice(&[0xFF, 0xD9]);
        v
    }

    fn jpeg(width: u16, height: u16) -> Vec<u8> {
        jpeg_with(0xC0, width, height, b"JFIF\0")
    }

    #[test]
    fn a_tiny_file_with_a_huge_header_is_refused() {
        let bomb = png(60_000, 60_000);
        assert!(bomb.len() < 100, "в файле меньше ста байт");
        assert_eq!(check_avatar(&bomb), Err(AvatarError::BadSize { width: 60_000, height: 60_000 }), "по размеру файла бомбу не отличить: надо смотреть на размеры в заголовке");
        assert_eq!(check_avatar(&jpeg(40_000, 40_000)), Err(AvatarError::BadSize { width: 40_000, height: 40_000 }), "то же для JPEG");
    }

    #[test]
    fn normal_sizes_pass_and_the_limits_are_exact() {
        assert_eq!(check_avatar(&png(256, 256)), Ok(Kind::Png), "обычный аватар");
        assert_eq!(check_avatar(&jpeg(1920, 1080)), Ok(Kind::Jpeg), "обычное фото");
        assert_eq!(check_avatar(&png(4000, 4000)), Ok(Kind::Png), "ровно 16 000 000 пикселей ещё можно");
        assert!(check_avatar(&png(4001, 4000)).is_err(), "16 004 000 пикселей уже много");
        assert_eq!(check_avatar(&png(8192, 1)), Ok(Kind::Png), "длинная и тонкая в пределах стороны");
        assert!(check_avatar(&png(8193, 1)).is_err(), "сторона на единицу больше предела");
        assert!(check_avatar(&png(0, 100)).is_err(), "нулевая ширина");
        assert!(check_avatar(&jpeg(100, 0)).is_err(), "нулевая высота");
    }

    #[test]
    fn the_area_does_not_overflow() {
        assert!(matches!(check_avatar(&png(65_536, 65_536)), Err(AvatarError::BadSize { .. })), "65536 на 65536 это 2^32 пикселей, а в u32 произведение превращается в ноль");
        assert!(matches!(check_avatar(&png(u32::MAX, u32::MAX)), Err(AvatarError::BadSize { .. })), "максимальные стороны не должны ронять проверку переполнением");
    }

    #[test]
    fn jpeg_markers_are_walked_by_their_lengths_not_searched_for() {
        let decoy = [0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x10, 0x00, 0x10, 0x03];
        let tricky = jpeg_with(0xC0, 40_000, 40_000, &decoy);
        assert_eq!(check_avatar(&tricky), Err(AvatarError::BadSize { width: 40_000, height: 40_000 }), "в комментарии внутри APP0 лежат байты FF C0, похожие на заголовок кадра: настоящий размер стоит после сегмента, который надо перешагнуть по его длине");
        assert_eq!(dimensions(&jpeg_with(0xC2, 800, 600, b"x"), Kind::Jpeg), Ok((800, 600)), "прогрессивный JPEG (SOF2) читается так же");
    }

    #[test]
    fn cut_headers_are_truncated_not_panics() {
        assert_eq!(check_avatar(&png(10, 10)[..20]), Err(AvatarError::Truncated), "PNG без конца IHDR");
        let full = jpeg(10, 10);
        assert_eq!(check_avatar(&full[..14]), Err(AvatarError::Truncated), "JPEG оборван до кадра");
        assert_eq!(check_avatar(&[0xFF, 0xD8, 0xFF]), Err(AvatarError::Truncated), "от JPEG только сигнатура");
    }

    #[test]
    fn webp_is_not_accepted_yet_and_other_checks_stay() {
        let mut webp = b"RIFF".to_vec();
        webp.extend_from_slice(&[0x24, 0, 0, 0]);
        webp.extend_from_slice(b"WEBPVP8 ");
        assert_eq!(check_avatar(&webp), Err(AvatarError::Unsupported), "WebP пока отвергаем: размеры его формата тут не разбираем");
        assert_eq!(check_avatar(b"<html></html>"), Err(AvatarError::NotImage), "не картинка");
        let mut big = png(10, 10);
        big.resize(MAX_BYTES + 1, 0);
        assert_eq!(check_avatar(&big), Err(AvatarError::TooBig), "размер файла по-прежнему ограничен");
    }
}
