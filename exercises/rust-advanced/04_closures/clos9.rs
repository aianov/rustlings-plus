// Три способа принять замыкание, и в каждой ситуации подходит свой. `apply_twice` вызывает функцию
// и отбрасывает её, `apply_each` получает сразу набор разных замыканий в одном срезе, а кнопка
// хранит обработчик в поле и может пережить место, где её создали. Тесты собирают кнопку в одной
// функции и нажимают в другой.
// TODO: Допиши `apply_twice`, `apply_each` и `Button::click`. Сигнатура `Button::new` написана не
// до конца: подумай, что нужно знать компилятору о замыкании, которое лежит внутри долгоживущей структуры.

pub fn apply_twice(f: impl Fn(i32) -> i32, x: i32) -> i32 {
    todo!("применить f к x два раза подряд")
}

pub fn apply_each(fs: &[&dyn Fn(i32) -> i32], x: i32) -> Vec<i32> {
    todo!("результат каждой функции для x")
}

pub struct Button {
    label: String,
    on_click: Box<dyn Fn(&str) -> String>,
}

impl Button {
    pub fn new(label: &str, on_click: impl Fn(&str) -> String) -> Button {
        Button { label: label.to_string(), on_click: Box::new(on_click) }
    }

    pub fn click(&self) -> String {
        todo!("вызвать обработчик с подписью кнопки")
    }
}

fn main() {
    println!("{}", apply_twice(|x| x + 1, 1));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twice_with_capturing_closure() {
        let step = 5;
        assert_eq!(apply_twice(|x| x + step, 1), 11);
        assert_eq!(apply_twice(|x| x * x, 3), 81);
    }

    #[test]
    fn each_takes_different_closures() {
        let offset = 100;
        let add = |x| x + offset;
        let neg = |x: i32| -x;
        fn triple(x: i32) -> i32 {
            x * 3
        }
        assert_eq!(apply_each(&[&add, &neg, &triple], 4), vec![104, -4, 12], "в одном срезе замыкания разных типов и обычная функция");
        assert!(apply_each(&[], 1).is_empty());
    }

    fn make_button() -> Button {
        let greeting = String::from("hello");
        Button::new("OK", move |label| format!("{greeting} from {label}"))
    }

    #[test]
    fn button_outlives_its_creator() {
        let button = make_button();
        assert_eq!(button.click(), "hello from OK", "кнопка помнит приветствие, хотя функция давно вернулась");
        assert_eq!(button.click(), "hello from OK");
    }
}
