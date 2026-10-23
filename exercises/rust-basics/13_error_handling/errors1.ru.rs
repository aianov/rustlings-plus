// TODO: Эта функция отказывается генерировать текст для бейджа, если передать
// ей пустую строку. Было бы приятнее, если бы она объясняла, в чём проблема,
// а не просто возвращала `None`. К счастью, в Rust есть похожая на `Option`
// конструкция для выражения ошибок. Измени сигнатуру и тело функции так,
// чтобы она возвращала `Result<String, String>` вместо
// `Option<String>`.
fn generate_nametag_text(name: String) -> Option<String> {
    if name.is_empty() {
        // Пустые имена не допускаются
        None
    } else {
        Some(format!("Hi! My name is {name}"))
    }
}

fn main() {
    // Здесь можно поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_nametag_text_for_a_nonempty_name() {
        assert_eq!(
            generate_nametag_text("Beyoncé".to_string()).as_deref(),
            Ok("Hi! My name is Beyoncé"),
        );
    }

    #[test]
    fn explains_why_generating_nametag_text_fails() {
        assert_eq!(
            generate_nametag_text(String::new())
                .as_ref()
                .map_err(|e| e.as_str()),
            Err("Empty names aren't allowed"),
        );
    }
}
