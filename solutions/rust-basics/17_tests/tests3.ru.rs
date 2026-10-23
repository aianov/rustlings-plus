struct Rectangle {
    width: i32,
    height: i32,
}

impl Rectangle {
    // Эту функцию не меняй.
    fn new(width: i32, height: i32) -> Self {
        if width <= 0 || height <= 0 {
            // Здесь лучше вернуть `Result`. Но мы хотим научиться тестировать
            // функции, которые могут паниковать.
            panic!("Ширина и высота прямоугольника должны быть положительными");
        }

        Rectangle { width, height }
    }
}

fn main() {
    // Здесь можно поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correct_width_and_height() {
        let rect = Rectangle::new(10, 20);
        assert_eq!(rect.width, 10); // Проверяем ширину
        assert_eq!(rect.height, 20); // Проверяем высоту
    }

    #[test]
    #[should_panic] // Добавили этот атрибут, чтобы проверить, что тест паникует.
    fn negative_width() {
        let _rect = Rectangle::new(-10, 10);
    }

    #[test]
    #[should_panic] // Добавили этот атрибут, чтобы проверить, что тест паникует.
    fn negative_height() {
        let _rect = Rectangle::new(10, -10);
    }
}
