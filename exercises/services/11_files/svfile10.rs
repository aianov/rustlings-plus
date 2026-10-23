// Ссылка на скачивание приватного файла не должна требовать входа в аккаунт (её открывает почтовый клиент, плеер,
// браузер друга), но и жить вечно не должна. Решение: ссылка несёт срок и подпись HMAC-SHA256, которую умеет ставить
// только сервер. Цель: `Signer::link` выдаёт `/files/<ключ>?exp=<срок>&sig=<hex>`, а `Signer::verify` по такой строке
// возвращает ключ файла. Ссылка действует, пока `now < exp`; любая правка ключа, срока или подписи, а равно чужой
// секрет, дают `BadSignature`; мусор вместо ссылки даёт `Malformed`. Подпись проверяется раньше срока: подделка не должна
// узнать, просрочена ли она.

use sha2::{Digest, Sha256};

fn main() {}

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

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Debug, PartialEq)]
pub enum LinkError {
    Malformed,
    BadSignature,
    Expired,
}

pub struct Signer {
    secret: Vec<u8>,
}

impl Signer {
    pub fn new(secret: &[u8]) -> Signer {
        Signer { secret: secret.to_vec() }
    }

    pub fn link(&self, key: &str, expires_at: u64) -> String {
        let _ = (key, expires_at, &self.secret, hmac_sha256(b"", b""), hex(&[]), ct_eq(&[], &[]));
        todo!()
    }

    pub fn verify(&self, path_and_query: &str, now: u64) -> Result<String, LinkError> {
        let _ = (path_and_query, now);
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signer() -> Signer {
        Signer::new(b"server secret")
    }

    fn param(link: &str, name: &str) -> String {
        link.split(['?', '&']).find_map(|p| p.strip_prefix(&format!("{name}="))).unwrap_or("").to_string()
    }

    #[test]
    fn a_fresh_link_opens_and_returns_the_key() {
        let link = signer().link("ab12cd", 2_000);
        assert!(link.starts_with("/files/ab12cd?exp=2000&sig="), "формат ссылки: {link}");
        assert_eq!(param(&link, "sig").len(), 64, "подпись это 64 hex-символа (32 байта)");
        assert_eq!(signer().verify(&link, 1_999), Ok("ab12cd".to_string()), "до срока ссылка открывается и отдаёт ключ");
    }

    #[test]
    fn the_expiry_is_exact() {
        let link = signer().link("ab12cd", 2_000);
        assert_eq!(signer().verify(&link, 2_000), Err(LinkError::Expired), "в момент exp ссылка уже не действует");
        assert_eq!(signer().verify(&link, 99_999), Err(LinkError::Expired), "и после тоже");
        assert!(signer().verify(&link, 0).is_ok(), "ссылка, выданная заранее, до срока открывается");
    }

    #[test]
    fn nothing_in_the_link_can_be_edited() {
        let link = signer().link("ab12cd", 2_000);
        let sig = param(&link, "sig");
        let extended = format!("/files/ab12cd?exp=9999999&sig={sig}");
        assert_eq!(signer().verify(&extended, 1_000), Err(LinkError::BadSignature), "срок продлили, подпись осталась: срок обязан входить в подпись");
        let other_file = format!("/files/zz99zz?exp=2000&sig={sig}");
        assert_eq!(signer().verify(&other_file, 1_000), Err(LinkError::BadSignature), "подпись одного файла не открывает другой");
        let flipped = format!("/files/ab12cd?exp=2000&sig={}{}", &sig[..63], if sig.ends_with('0') { '1' } else { '0' });
        assert_eq!(signer().verify(&flipped, 1_000), Err(LinkError::BadSignature), "один изменённый символ подписи");
    }

    #[test]
    fn another_secret_does_not_open_it() {
        let link = Signer::new(b"other secret").link("ab12cd", 2_000);
        assert_eq!(signer().verify(&link, 1_000), Err(LinkError::BadSignature), "ссылку с чужим секретом сервер не признаёт");
    }

    #[test]
    fn a_forged_link_does_not_reveal_whether_it_is_expired() {
        let forged = format!("/files/ab12cd?exp=5&sig={}", "0".repeat(64));
        assert_eq!(signer().verify(&forged, 1_000), Err(LinkError::BadSignature), "поддельная ссылка с прошлым сроком получает BadSignature: подпись проверяется раньше срока");
    }

    #[test]
    fn garbage_is_malformed() {
        let sig = "a".repeat(64);
        for bad in [
            "".to_string(),
            "/files/".to_string(),
            "/files/ab12cd".to_string(),
            "/other/ab12cd?exp=5&sig=".to_string() + &sig,
            format!("/files/ab12cd?exp=abc&sig={sig}"),
            format!("/files/ab12cd?exp=5&sig={}", &sig[..10]),
            format!("/files/ab12cd?exp=5&sig={}", "G".repeat(64)),
            format!("/files/ab12cd?exp=5&sig={}", "A".repeat(64)),
            format!("/files/../etc/passwd?exp=5&sig={sig}"),
            format!("/files/a%2Fb?exp=5&sig={sig}"),
        ] {
            assert_eq!(signer().verify(&bad, 1), Err(LinkError::Malformed), "строка {bad:?} не ссылка нужного вида");
        }
    }

    #[test]
    fn keys_with_odd_characters_are_not_signed_at_all() {
        let link = signer().link("a/b", 10);
        assert_eq!(signer().verify(&link, 1), Err(LinkError::Malformed), "ссылка, в которой ключ с косой чертой, не должна открываться ни при каких подписях");
    }
}
