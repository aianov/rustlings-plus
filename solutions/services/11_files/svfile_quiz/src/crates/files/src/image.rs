pub const MAX_BYTES: usize = 1024 * 1024;
pub const MAX_SIDE: u32 = 8192;
pub const MAX_PIXELS: u64 = 16_000_000;

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

fn be32(bytes: &[u8], at: usize) -> Result<u32, AvatarError> {
    let raw = bytes.get(at..at + 4).ok_or(AvatarError::Truncated)?;
    Ok(u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn be16(bytes: &[u8], at: usize) -> Result<usize, AvatarError> {
    let raw = bytes.get(at..at + 2).ok_or(AvatarError::Truncated)?;
    Ok(usize::from(u16::from_be_bytes([raw[0], raw[1]])))
}

fn png_size(bytes: &[u8]) -> Result<(u32, u32), AvatarError> {
    if bytes.len() < 24 || bytes[12..16] != b"IHDR"[..] {
        return Err(AvatarError::Truncated);
    }
    Ok((be32(bytes, 16)?, be32(bytes, 20)?))
}

fn jpeg_size(bytes: &[u8]) -> Result<(u32, u32), AvatarError> {
    let mut at = 2;
    loop {
        if *bytes.get(at).ok_or(AvatarError::Truncated)? != 0xFF {
            return Err(AvatarError::Truncated);
        }
        let mut marker = *bytes.get(at + 1).ok_or(AvatarError::Truncated)?;
        at += 1;
        while marker == 0xFF {
            at += 1;
            marker = *bytes.get(at).ok_or(AvatarError::Truncated)?;
        }
        at += 1;
        match marker {
            0x01 | 0xD0..=0xD7 => continue,
            0xD9 | 0xDA => return Err(AvatarError::Truncated),
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                let height = be16(bytes, at + 3)?;
                let width = be16(bytes, at + 5)?;
                return Ok((width as u32, height as u32));
            }
            _ => at += be16(bytes, at)?,
        }
    }
}

pub fn dimensions(bytes: &[u8], kind: Kind) -> Result<(u32, u32), AvatarError> {
    match kind {
        Kind::Png => png_size(bytes),
        Kind::Jpeg => jpeg_size(bytes),
        Kind::WebP => Err(AvatarError::Unsupported),
    }
}

pub fn check_avatar(bytes: &[u8]) -> Result<Kind, AvatarError> {
    if bytes.len() > MAX_BYTES {
        return Err(AvatarError::TooBig);
    }
    let kind = sniff(bytes).ok_or(AvatarError::NotImage)?;
    let (width, height) = dimensions(bytes, kind)?;
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(AvatarError::BadSize { width, height });
    }
    Ok(kind)
}
