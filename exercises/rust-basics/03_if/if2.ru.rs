// TODO: Исправь ошибку компилятора в этой функции.
fn picky_eater(food: &str) -> &str {
    if food == "strawberry" {
        "Yummy!"
    } else {
        1
    }
}

fn main() {
    // Здесь можно по желанию поэкспериментировать.
}

// TODO: Прочитай тесты, чтобы понять нужное поведение.
// Добейся, чтобы все тесты проходили, не меняя их.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yummy_food() {
        // Это значит, что вызов `picky_eater` с аргументом "strawberry" должен вернуть "Yummy!".
        assert_eq!(picky_eater("strawberry"), "Yummy!");
    }

    #[test]
    fn neutral_food() {
        assert_eq!(picky_eater("potato"), "I guess I can eat that.");
    }

    #[test]
    fn default_disliked_food() {
        assert_eq!(picky_eater("broccoli"), "No thanks!");
        assert_eq!(picky_eater("gummy bears"), "No thanks!");
        assert_eq!(picky_eater("literally anything"), "No thanks!");
    }
}
