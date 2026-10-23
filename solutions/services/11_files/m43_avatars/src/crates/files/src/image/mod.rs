use std::fmt;

pub const MAX_SIDE: u32 = 4096;

pub const MAX_PIXELS: u64 = 16_000_000;

pub const HEAD_MAX: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Png,
    Jpeg,
    Webp,
}

impl Kind {
    pub fn mime(self) -> &'static str {
        match self {
            Kind::Png => "image/png",
            Kind::Jpeg => "image/jpeg",
            Kind::Webp => "image/webp",
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            Kind::Png => "png",
            Kind::Jpeg => "jpg",
            Kind::Webp => "webp",
        }
    }

    pub fn parse(mime: &str) -> Option<Kind> {
        [Kind::Png, Kind::Jpeg, Kind::Webp]
            .into_iter()
            .find(|kind| kind.mime() == mime)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Picture {
    pub kind: Kind,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageError {
    NotAnImage,
    Truncated,
    BadSize { width: u32, height: u32 },
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageError::NotAnImage => write!(f, "это не PNG, JPEG и не WebP"),
            ImageError::Truncated => write!(f, "заголовок картинки оборван"),
            ImageError::BadSize { width, height } => write!(
                f,
                "картинка {width}×{height}: стороны от 1 до {MAX_SIDE}, площадь не больше {MAX_PIXELS}"
            ),
        }
    }
}

impl std::error::Error for ImageError {}

pub fn sniff(head: &[u8]) -> Option<Kind> {
    if head.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        return Some(Kind::Png);
    }
    if head.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some(Kind::Jpeg);
    }
    if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        return Some(Kind::Webp);
    }
    None
}

fn be16(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_be_bytes([
        *bytes.get(at)?,
        *bytes.get(at + 1)?,
    ])))
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    let slice = bytes.get(at..at + 4)?;
    Some(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn le24(bytes: &[u8], at: usize) -> Option<u32> {
    let slice = bytes.get(at..at + 3)?;
    Some(u32::from(slice[0]) | u32::from(slice[1]) << 8 | u32::from(slice[2]) << 16)
}

fn png(head: &[u8]) -> Option<(u32, u32)> {
    if head.get(12..16)? != b"IHDR" {
        return None;
    }
    Some((be32(head, 16)?, be32(head, 20)?))
}

fn jpeg(head: &[u8]) -> Option<(u32, u32)> {
    let mut at = 2;
    loop {
        while *head.get(at)? == 0xff && *head.get(at + 1)? == 0xff {
            at += 1;
        }
        if *head.get(at)? != 0xff {
            return None;
        }
        let marker = *head.get(at + 1)?;
        let length = usize::try_from(be16(head, at + 2)?).ok()?;
        let frame = (0xc0..=0xcf).contains(&marker) && ![0xc4, 0xc8, 0xcc].contains(&marker);
        if frame {
            return Some((be16(head, at + 7)?, be16(head, at + 5)?));
        }
        at += 2 + length;
    }
}

fn webp(head: &[u8]) -> Option<(u32, u32)> {
    match head.get(12..16)? {
        b"VP8X" => Some((le24(head, 24)? + 1, le24(head, 27)? + 1)),
        b"VP8 " => {
            let width = u32::from(u16::from_le_bytes([*head.get(26)?, *head.get(27)?])) & 0x3fff;
            let height = u32::from(u16::from_le_bytes([*head.get(28)?, *head.get(29)?])) & 0x3fff;
            Some((width, height))
        }
        b"VP8L" => {
            let bits = u32::from_le_bytes([
                *head.get(21)?,
                *head.get(22)?,
                *head.get(23)?,
                *head.get(24)?,
            ]);
            Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
        }
        _ => None,
    }
}

pub fn check_avatar(head: &[u8]) -> Result<Picture, ImageError> {
    let kind = match sniff(head) {
        Some(kind) => kind,
        None if head.len() < 12 => return Err(ImageError::Truncated),
        None => return Err(ImageError::NotAnImage),
    };
    let size = match kind {
        Kind::Png => png(head),
        Kind::Jpeg => jpeg(head),
        Kind::Webp => webp(head),
    };
    let (width, height) = size.ok_or(ImageError::Truncated)?;
    let area = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE || area > MAX_PIXELS {
        return Err(ImageError::BadSize { width, height });
    }
    Ok(Picture {
        kind,
        width,
        height,
    })
}
