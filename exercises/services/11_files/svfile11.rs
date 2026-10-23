// Ссылки на скачивание получили параметр `name`: под каким именем браузер сохранит файл. И параметр `exp`, срок. Подпись,
// как её сделали в спешке, закрывает только ключ файла. Любопытный пользователь продлил свою ссылку, поменяв число в
// URL, а другой подставил `name=invoice.exe`, и сервер честно предложил скачать картинку под таким именем. Цель: ссылка
// `/files/<ключ>?exp=<срок>&name=<имя>&sig=<hex>` защищена целиком: изменение ключа, срока или имени даёт `BadSignature`,
// и никакие два разных набора (ключ, срок, имя) не могут иметь одну и ту же подписываемую строку.

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

#[derive(Debug, PartialEq)]
pub struct Opened {
    pub key: String,
    pub name: String,
}

pub struct Signer {
    secret: Vec<u8>,
}

fn safe(text: &str) -> bool {
    !text.is_empty() && text.len() <= 128 && text.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn mac(secret: &[u8], key: &str, _exp: u64, _name: &str) -> String {
    hex(&hmac_sha256(secret, key.as_bytes()))
}

impl Signer {
    pub fn new(secret: &[u8]) -> Signer {
        Signer { secret: secret.to_vec() }
    }

    pub fn link(&self, key: &str, expires_at: u64, name: &str) -> String {
        format!("/files/{key}?exp={expires_at}&name={name}&sig={}", mac(&self.secret, key, expires_at, name))
    }

    pub fn verify(&self, path_and_query: &str, now: u64) -> Result<Opened, LinkError> {
        let rest = path_and_query.strip_prefix("/files/").ok_or(LinkError::Malformed)?;
        let (key, query) = rest.split_once('?').ok_or(LinkError::Malformed)?;
        let query = query.strip_prefix("exp=").ok_or(LinkError::Malformed)?;
        let (exp, query) = query.split_once("&name=").ok_or(LinkError::Malformed)?;
        let (name, sig) = query.split_once("&sig=").ok_or(LinkError::Malformed)?;
        let exp: u64 = exp.parse().map_err(|_| LinkError::Malformed)?;
        if !safe(key) || !safe(name) || sig.len() != 64 || !sig.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return Err(LinkError::Malformed);
        }
        if !ct_eq(sig.as_bytes(), mac(&self.secret, key, exp, name).as_bytes()) {
            return Err(LinkError::BadSignature);
        }
        if now >= exp {
            return Err(LinkError::Expired);
        }
        Ok(Opened { key: key.to_string(), name: name.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signer() -> Signer {
        Signer::new(b"server secret")
    }

    fn sig_of(link: &str) -> String {
        link.rsplit("&sig=").next().unwrap().to_string()
    }

    #[test]
    fn an_honest_link_opens_with_the_key_and_the_name() {
        let link = signer().link("ab12cd", 2_000, "cat.png");
        assert_eq!(signer().verify(&link, 1_000), Ok(Opened { key: "ab12cd".into(), name: "cat.png".into() }), "честная ссылка открывается");
        assert_eq!(signer().verify(&link, 2_000), Err(LinkError::Expired), "в момент срока уже нет");
    }

    #[test]
    fn extending_the_expiry_breaks_the_signature() {
        let link = signer().link("ab12cd", 2_000, "cat.png");
        let forged = format!("/files/ab12cd?exp=9999999&name=cat.png&sig={}", sig_of(&link));
        assert_eq!(signer().verify(&forged, 1_000), Err(LinkError::BadSignature), "срок продлили вручную: подпись должна покрывать срок");
    }

    #[test]
    fn changing_the_download_name_breaks_the_signature() {
        let link = signer().link("ab12cd", 2_000, "cat.png");
        let forged = format!("/files/ab12cd?exp=2000&name=invoice.exe&sig={}", sig_of(&link));
        assert_eq!(signer().verify(&forged, 1_000), Err(LinkError::BadSignature), "имя для скачивания подменили: подпись должна покрывать имя");
    }

    #[test]
    fn another_file_with_the_same_signature_is_refused() {
        let link = signer().link("ab12cd", 2_000, "cat.png");
        let forged = format!("/files/ff00ff?exp=2000&name=cat.png&sig={}", sig_of(&link));
        assert_eq!(signer().verify(&forged, 1_000), Err(LinkError::BadSignature), "подпись привязана к ключу");
    }

    #[test]
    fn fields_cannot_be_shifted_between_each_other() {
        let link = signer().link("ab", 12, "3.png");
        let shifted = format!("/files/ab1?exp=2&name=3.png&sig={}", sig_of(&link));
        assert_eq!(signer().verify(&shifted, 1), Err(LinkError::BadSignature), "ключ ab, срок 12 и ключ ab1, срок 2 не должны давать одну подписываемую строку: между полями нужен разделитель, которого нет в самих полях");
        let link = signer().link("ab", 123, "x.png");
        let shifted = format!("/files/ab?exp=12&name=3x.png&sig={}", sig_of(&link));
        assert_eq!(signer().verify(&shifted, 1), Err(LinkError::BadSignature), "срок 123 с именем x.png и срок 12 с именем 3x.png тоже должны различаться");
    }

    #[test]
    fn a_forged_expired_link_is_a_bad_signature_not_expired() {
        let forged = format!("/files/ab12cd?exp=5&name=a.png&sig={}", "0".repeat(64));
        assert_eq!(signer().verify(&forged, 1_000), Err(LinkError::BadSignature), "подпись проверяется раньше срока");
    }

    #[test]
    fn broken_links_are_malformed() {
        let sig = "a".repeat(64);
        for bad in [
            format!("/files/ab12cd?exp=5&sig={sig}"),
            format!("/files/ab12cd?exp=x&name=a.png&sig={sig}"),
            format!("/files/ab12cd?exp=5&name=a/b.png&sig={sig}"),
            format!("/files/ab12cd?exp=5&name=&sig={sig}"),
            format!("/files/ab12cd?exp=5&name=a.png&sig={}", &sig[..5]),
        ] {
            assert_eq!(signer().verify(&bad, 1), Err(LinkError::Malformed), "{bad:?} не похоже на ссылку");
        }
    }
}
