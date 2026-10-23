trait Licensed {
    // TODO: Добавь реализацию по умолчанию для `licensing_info`, чтобы
    // реализующие типы, например две структуры ниже, могли делить это поведение
    // и не повторять функцию.
    // Информация о лицензии по умолчанию: строка "Default license".
    fn licensing_info(&self) -> String;
}

struct SomeSoftware {
    version_number: i32,
}

struct OtherSoftware {
    version_number: String,
}

impl Licensed for SomeSoftware {} // Эту строку не меняй.
impl Licensed for OtherSoftware {} // Эту строку не меняй.

fn main() {
    // Здесь можно поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_licensing_info_the_same() {
        let licensing_info = "Default license";
        let some_software = SomeSoftware { version_number: 1 };
        let other_software = OtherSoftware {
            version_number: "v2.0.0".to_string(),
        };
        assert_eq!(some_software.licensing_info(), licensing_info);
        assert_eq!(other_software.licensing_info(), licensing_info);
    }
}
