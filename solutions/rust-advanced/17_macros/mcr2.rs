#![allow(unused_macros)]

macro_rules! ensure {
    ($cond:expr, $msg:expr) => {
        if !$cond {
            return Err($msg.to_string());
        }
    };
    ($cond:expr) => {
        if !$cond {
            return Err(format!("check failed: {}", stringify!($cond)));
        }
    };
}

macro_rules! parse_as {
    ($text:expr => $t:ty) => {
        $text.trim().parse::<$t>().map_err(|e| e.to_string())
    };
}

fn validate_age(age: i32) -> Result<i32, String> {
    ensure!(age > 0, "возраст должен быть положительным");
    ensure!(age < 150);
    Ok(age)
}

fn main() {
    println!("{:?}", validate_age(30));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_returns_the_given_message() {
        assert_eq!(validate_age(-1), Err("возраст должен быть положительным".to_string()));
        assert_eq!(validate_age(30), Ok(30));
    }

    #[test]
    fn ensure_without_a_message_quotes_the_condition() {
        assert_eq!(validate_age(200), Err("check failed: age < 150".to_string()), "текст условия берётся через stringify!");
    }

    #[test]
    fn ensure_message_may_be_any_expression() {
        fn check(n: u8) -> Result<u8, String> {
            ensure!(n % 2 == 0, format!("{n} нечётно"));
            Ok(n)
        }
        assert_eq!(check(3), Err("3 нечётно".to_string()));
        assert_eq!(check(4), Ok(4));
    }

    #[test]
    fn parse_as_trims_and_converts() {
        assert_eq!(parse_as!(" 42 " => u8), Ok(42));
        assert!(parse_as!("300" => u8).is_err());
        assert_eq!(parse_as!("true" => bool), Ok(true));
        assert_eq!(parse_as!(String::from("7") => i64), Ok(7));
    }
}
