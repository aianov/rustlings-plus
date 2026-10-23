// В блоке два `?` с ошибками разных типов, и оба должны превратиться в ProfileError через From.
// Обычная функция знает свой тип ошибки из сигнатуры, а у async-блока сигнатуры нет, и
// компилятор не может понять, во что собирать ошибку блока. Сейчас код не собирается.
// TODO: Сделай так, чтобы load_age собралась и вернула правильные ошибки.
use std::num::ParseIntError;
use std::time::Duration;

#[derive(Debug, PartialEq)]
enum ProfileError {
    BadAge(String),
    TooYoung(u32),
}

struct Underage(u32);

impl From<ParseIntError> for ProfileError {
    fn from(error: ParseIntError) -> Self {
        ProfileError::BadAge(error.to_string())
    }
}

impl From<Underage> for ProfileError {
    fn from(error: Underage) -> Self {
        ProfileError::TooYoung(error.0)
    }
}

fn ensure_adult(age: u32) -> Result<u32, Underage> {
    if age < 14 { Err(Underage(age)) } else { Ok(age) }
}

async fn load_age(raw: &str) -> Result<u32, ProfileError> {
    let job = async {
        tokio::time::sleep(Duration::from_millis(1)).await;
        let age: u32 = raw.trim().parse()?;
        let age = ensure_adult(age)?;
        Ok(age)
    };
    let age = job.await?;
    Ok(age)
}

#[tokio::main]
async fn main() {
    println!("{:?}", load_age(" 42 ").await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn good_age() {
        assert_eq!(load_age(" 25 ").await, Ok(25), "пробелы по краям надо обрезать");
    }

    #[tokio::test]
    async fn garbage_becomes_bad_age() {
        let result = load_age("abc").await;
        assert!(
            matches!(result, Err(ProfileError::BadAge(_))),
            "ошибка разбора должна превратиться в ProfileError::BadAge, а вышло {result:?}"
        );
    }

    #[tokio::test]
    async fn too_young() {
        assert_eq!(load_age("10").await, Err(ProfileError::TooYoung(10)), "младше 14 лет нельзя");
    }
}
