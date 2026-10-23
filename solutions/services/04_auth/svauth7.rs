
use sha2::{Digest, Sha256};

pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(block.map(|b| b ^ 0x36));
    inner.update(msg);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(block.map(|b| b ^ 0x5c));
    outer.update(inner_hash);
    outer.finalize().into()
}

pub fn verify(key: &[u8], msg: &[u8], tag: &[u8]) -> bool {
    let expected = hmac_sha256(key, msg);
    tag.len() == expected.len() && tag.iter().zip(expected.iter()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: [u8; 32]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn rfc_4231_long_key_cases() {
        assert_eq!(
            hex(hmac_sha256(&[0xaa; 131], b"Test Using Larger Than Block-Size Key - Hash Key First")),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54",
            "случай 6: ключ длиннее блока сначала хешируется"
        );
        assert_eq!(
            hex(hmac_sha256(
                &[0xaa; 131],
                b"This is a test using a larger than block-size key and a larger than block-size data. The key needs to be hashed before being used by the HMAC algorithm."
            )),
            "9b09ffa71b942fcb27635fbcd5b0e944bfdc63644f0713938a7f51535c3a35e2",
            "случай 7"
        );
    }

    #[test]
    fn keys_that_share_a_64_byte_prefix_are_different_keys() {
        let mut other = vec![0xaa; 131];
        other[100] = 0xab;
        assert_ne!(
            hmac_sha256(&[0xaa; 131], b"m"),
            hmac_sha256(&other, b"m"),
            "ключи различаются после 64-го байта, значит, подписи тоже должны различаться"
        );
    }

    #[test]
    fn short_keys_still_work() {
        assert_eq!(
            hex(hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
            "короткий ключ"
        );
    }

    #[test]
    fn verify_accepts_only_the_full_tag() {
        let tag = hmac_sha256(b"key", b"message");
        assert!(verify(b"key", b"message", &tag), "верная подпись");
        assert!(!verify(b"key", b"message2", &tag), "другое сообщение");
        assert!(!verify(b"key2", b"message", &tag), "другой ключ");
        assert!(!verify(b"key", b"message", &[]), "пустая подпись не может проходить");
        assert!(!verify(b"key", b"message", &tag[..1]), "один верный байт не подпись");
        assert!(!verify(b"key", b"message", &tag[..31]), "обрезанная подпись");
        let mut longer = tag.to_vec();
        longer.push(0);
        assert!(!verify(b"key", b"message", &longer), "подпись с хвостом");
    }
}
