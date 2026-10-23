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
        // TODO: Этот тест должен проверять, что у прямоугольника размер,
        // который мы передали конструктору.
        let rect = Rectangle::new(10, 20);
        assert_eq!(todo!(), 10); // Проверяем ширину
        assert_eq!(todo!(), 20); // Проверяем высоту
    }

    // TODO: Этот тест должен проверять, что программа паникует при попытке
    // создать прямоугольник с отрицательной шириной.
    #[test]
    fn negative_width() {
        let _rect = Rectangle::new(-10, 10);
    }

    // TODO: Этот тест должен проверять, что программа паникует при попытке
    // создать прямоугольник с отрицательной высотой.
    #[test]
    fn negative_height() {
        let _rect = Rectangle::new(10, -10);
    }
}
