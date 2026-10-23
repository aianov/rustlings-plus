// Пароль базы лежит в конфиге, а конфиг печатают в лог при старте через `{:?}`. Так секреты и
// утекают: в логах их читают все, кому дали доступ к логам. Обёртка `Secret<T>` решает проблему
// типом: напечатать её нечаянно нельзя, достать значение можно только явным вызовом.
// TODO: Допиши `Secret`: `Debug` печатает `Secret(***)`, `Display` печатает `***`,
// значение достаётся через `expose`.

use std::fmt;

pub struct Secret<T>(T);

impl<T> Secret<T> {
    pub fn new(value: T) -> Self {
        todo!()
    }

    pub fn expose(&self) -> &T {
        todo!()
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl<T> fmt::Display for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

#[derive(Debug)]
pub struct DbConfig {
    pub host: String,
    pub user: String,
    pub password: Secret<String>,
}

fn main() {
    let cfg = DbConfig {
        host: "db".to_string(),
        user: "app".to_string(),
        password: Secret::new("hunter2".to_string()),
    };
    println!("{cfg:?}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> DbConfig {
        DbConfig {
            host: "db.internal".to_string(),
            user: "app".to_string(),
            password: Secret::new("hunter2".to_string()),
        }
    }

    #[test]
    fn debug_hides_the_value() {
        assert_eq!(format!("{:?}", Secret::new("hunter2")), "Secret(***)", "Debug печатает Secret(***)");
        let text = format!("{:?}", config());
        assert!(!text.contains("hunter2"), "пароль утёк в Debug всего конфига: {text}");
        assert!(text.contains("db.internal"), "остальные поля конфига печатаются как обычно: {text}");
    }

    #[test]
    fn pretty_debug_hides_the_value_too() {
        let text = format!("{:#?}", config());
        assert!(!text.contains("hunter2"), "пароль утёк в {{:#?}}: {text}");
    }

    #[test]
    fn display_hides_the_value() {
        let secret = Secret::new(String::from("hunter2"));
        assert_eq!(format!("{secret}"), "***", "Display печатает три звёздочки");
        assert_eq!(format!("password={secret}"), "password=***", "и в строке с форматом тоже");
    }

    #[test]
    fn expose_is_the_only_way_out() {
        let cfg = config();
        assert_eq!(cfg.password.expose(), "hunter2", "значение достаётся явным вызовом");
        let numbers = Secret::new(vec![1, 2, 3]);
        assert_eq!(numbers.expose().len(), 3, "обёртка работает для любого типа");
    }
}
