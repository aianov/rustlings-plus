// Подпись токена это HMAC-SHA256: хеш от ключа и сообщения, устроенный хитрее, чем `sha256(ключ + сообщение)`
// (тот ломается атакой удлинения). Крейт `hmac` мы не берём, алгоритм на двадцать строк поверх `sha2`:
// ключ дополняется нулями до блока в 64 байта, внутренний хеш считается от `ключ XOR 0x36` и сообщения,
// внешний от `ключ XOR 0x5c` и внутреннего хеша. Цель: `hmac_sha256` сходится с тестовыми векторами RFC 4231.

use sha2::{Digest, Sha256};

pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut inner = Sha256::new();
    inner.update(key);
    inner.update(msg);
    inner.finalize().into()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: [u8; 32]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn rfc_4231_case_1() {
        assert_eq!(
            hex(hmac_sha256(&[0x0b; 20], b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
            "случай 1 из RFC 4231"
        );
    }

    #[test]
    fn rfc_4231_case_2_short_key() {
        assert_eq!(
            hex(hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
            "случай 2: ключ короче блока дополняется нулями"
        );
    }

    #[test]
    fn rfc_4231_cases_3_and_4() {
        assert_eq!(
            hex(hmac_sha256(&[0xaa; 20], &[0xdd; 50])),
            "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe",
            "случай 3"
        );
        let key: Vec<u8> = (1..=25).collect();
        assert_eq!(
            hex(hmac_sha256(&key, &[0xcd; 50])),
            "82558a389a443c0ea4cc819899f2083a85f0faa3e578f8077a2e3ff46729665b",
            "случай 4"
        );
    }

    #[test]
    fn different_keys_or_messages_give_different_tags() {
        assert_ne!(hmac_sha256(b"k1", b"m"), hmac_sha256(b"k2", b"m"), "другой ключ");
        assert_ne!(hmac_sha256(b"k", b"m1"), hmac_sha256(b"k", b"m2"), "другое сообщение");
    }
}
