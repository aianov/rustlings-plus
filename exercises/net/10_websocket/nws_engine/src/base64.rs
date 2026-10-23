const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base64Error {
    BadLength,
    BadChar(u8),
    BadPadding,
    NonZeroTail,
}

pub fn encode(data: &[u8]) -> String {
    todo!()
}

pub fn decode(text: &str) -> Result<Vec<u8>, Base64Error> {
    todo!()
}
