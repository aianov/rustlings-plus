// Год назад выбрали мягкие параметры argon2, сегодня железо ускорилось и параметры подняли. В базе
// лежат записи со старыми параметрами, и нет смысла просить всех сменить пароль. Но при входе мы
// как раз знаем пароль в открытом виде: самое время пересчитать запись по новым параметрам. Цель:
// `login_check` проверяет пароль и сообщает, нужна ли новая запись (если параметры записи слабее
// целевых). Неверный пароль ничего не обновляет, мусор вместо записи не паникует.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, SaltString};
use argon2::{Algorithm, Argon2, Params, PasswordHasher, PasswordVerifier, Version};

#[derive(Debug, PartialEq)]
pub enum LoginError {
    WrongPassword,
    Malformed,
}

pub fn hasher(params: &Params) -> Argon2<'static> {
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params.clone())
}

pub fn hash_with(password: &str, params: &Params) -> String {
    let salt = SaltString::generate(&mut OsRng);
    hasher(params).hash_password(password.as_bytes(), &salt).expect("хеширование").to_string()
}

pub fn login_check(password: &str, stored: &str, target: &Params) -> Result<Option<String>, LoginError> {
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn weak() -> Params {
        Params::new(8, 1, 1, None).unwrap()
    }

    fn strong() -> Params {
        Params::new(64, 2, 1, None).unwrap()
    }

    fn params_of(stored: &str) -> Params {
        Params::try_from(&PasswordHash::new(stored).unwrap()).unwrap()
    }

    #[test]
    fn old_record_is_upgraded_on_successful_login() {
        let stored = hash_with("hunter22", &weak());
        let upgraded = login_check("hunter22", &stored, &strong()).expect("пароль верный").expect("запись слабее целевых параметров, нужна новая");
        assert_eq!(params_of(&upgraded).m_cost(), 64, "память новой записи по целевым параметрам");
        assert_eq!(params_of(&upgraded).t_cost(), 2, "число проходов по целевым параметрам");
        assert!(login_check("hunter22", &upgraded, &strong()).is_ok(), "новая запись принимает тот же пароль");
    }

    #[test]
    fn up_to_date_record_is_left_alone() {
        let stored = hash_with("hunter22", &strong());
        assert_eq!(login_check("hunter22", &stored, &strong()), Ok(None), "параметры уже целевые: перехешировать не нужно");
    }

    #[test]
    fn stronger_record_is_not_downgraded() {
        let stored = hash_with("hunter22", &strong());
        assert_eq!(login_check("hunter22", &stored, &weak()), Ok(None), "запись сильнее целевой: не ослабляем");
    }

    #[test]
    fn wrong_password_upgrades_nothing() {
        let stored = hash_with("hunter22", &weak());
        assert_eq!(login_check("wrong", &stored, &strong()), Err(LoginError::WrongPassword), "неверный пароль: никакой новой записи");
    }

    #[test]
    fn every_parameter_counts() {
        let stored = hash_with("pw", &Params::new(64, 2, 1, None).unwrap());
        let more_passes = Params::new(64, 3, 1, None).unwrap();
        assert!(login_check("pw", &stored, &more_passes).unwrap().is_some(), "выросло только число проходов, этого достаточно");
        let more_lanes = Params::new(64, 2, 2, None).unwrap();
        assert!(login_check("pw", &stored, &more_lanes).unwrap().is_some(), "выросла только параллельность");
    }

    #[test]
    fn junk_record_is_malformed_not_a_panic() {
        assert_eq!(login_check("x", "plaintext", &strong()), Err(LoginError::Malformed), "не PHC-запись");
        assert_eq!(login_check("x", "", &strong()), Err(LoginError::Malformed), "пустая запись");
    }
}
