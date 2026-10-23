// Утечка базы: у стартапа пароли лежат как sha256 без соли. Атакующий за секунду прогнал список
// популярных паролей, посчитал sha256 каждого и сравнил с таблицей: нашёл половину учёток. Одинаковые
// пароли у разных людей видны сразу, потому что хеши совпадают. Цель: хранить пароли так, чтобы
// подбор по словарю шёл дорого (медленный алгоритм с памятью) и каждая запись была уникальной
// даже для одинаковых паролей. Проверка пароля при этом работает, а на мусоре вместо записи не паникует.

use argon2::{Algorithm, Argon2, Params, Version};
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, SaltString};
use argon2::{PasswordHasher, PasswordVerifier};
use sha2::{Digest, Sha256};

fn argon() -> Argon2<'static> {
    Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::new(8, 1, 1, None).expect("параметры"))
}

pub fn hash_password(password: &str) -> String {
    format!("{:x}", Sha256::digest(password.as_bytes()))
}

pub fn verify_password(password: &str, stored: &str) -> bool {
    hash_password(password) == stored
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn right_password_passes_and_wrong_fails() {
        let stored = hash_password("correct horse");
        assert!(verify_password("correct horse", &stored), "верный пароль должен проходить");
        assert!(!verify_password("correct horsf", &stored), "неверный пароль отклоняется");
        assert!(!verify_password("", &stored), "пустой пароль тоже");
    }

    #[test]
    fn same_password_gives_different_records() {
        let a = hash_password("123456");
        let b = hash_password("123456");
        assert_ne!(a, b, "у двух пользователей с одним паролем записи должны отличаться: нужна своя соль у каждой записи");
        assert!(verify_password("123456", &a) && verify_password("123456", &b), "но проверка работает для обеих");
    }

    #[test]
    fn dictionary_attack_with_plain_sha256_finds_nothing() {
        let leaked: Vec<String> = ["123456", "password", "qwerty", "letmein"].iter().map(|p| hash_password(p)).collect();
        let dictionary = ["123456", "password", "qwerty", "letmein", "dragon", "monkey"];
        let mut cracked = Vec::new();
        for word in dictionary {
            let guess = format!("{:x}", Sha256::digest(word.as_bytes()));
            if leaked.contains(&guess) {
                cracked.push(word);
            }
        }
        assert!(cracked.is_empty(), "по таблице хешей словарь без соли нашёл пароли: {cracked:?}");
    }

    #[test]
    fn record_is_self_describing() {
        let stored = hash_password("x");
        assert!(stored.starts_with("$argon2id$"), "запись в формате PHC несёт алгоритм, параметры и соль: {stored}");
        let parts: Vec<&str> = stored.split('$').collect();
        assert!(parts.len() == 6 && parts[4].len() >= 8 && parts[5].len() >= 16, "в записи должны быть и соль, и результат: {stored}");
    }

    #[test]
    fn garbage_instead_of_a_record_is_just_a_failure() {
        for junk in ["", "plaintext", "$argon2id$", "5f4dcc3b5aa765d61d8327deb882cf99"] {
            assert!(!verify_password("password", junk), "на записи {junk:?} проверка должна вернуть false, а не паниковать");
        }
    }
}
