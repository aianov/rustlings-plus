// Библиотека рендеринга хотела две общие реализации: для всего, что умеет `Display`, и для вектора
// всего, что умеет `Debug`. Каждая по отдельности нормальная, вместе они конфликтуют: у `Vec<T>`
// может быть `Display`, если когда-нибудь появится реализация (а `Vec<T>` не наш тип, и она может
// появиться в стандартной библиотеке), и компилятор не может доказать, что обе реализации никогда не
// пересекутся («conflicting implementations»). Привычный выход: вынести вторую логику в тип-обёртку-маркер,
// и общая реализация для маркера уже не пересекается с первой.
// Формат: для `Display` это `[значение]`; для обёрнутого `Debugged(x)` это `{x:?}` в фигурных скобках `{...}`.
// TODO: Почини реализации и добавь `Debugged`.

use std::fmt::{Debug, Display};

pub trait Render {
    fn render(&self) -> String;
}

impl<T: Display> Render for T {
    fn render(&self) -> String {
        format!("[{self}]")
    }
}

impl<T: Debug> Render for Vec<T> {
    fn render(&self) -> String {
        format!("{{{self:?}}}")
    }
}

fn main() {
    println!("{}", 5.render());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_types() {
        assert_eq!(5.render(), "[5]");
        assert_eq!("hi".render(), "[hi]");
        assert_eq!(String::from("s").render(), "[s]");
    }

    #[test]
    fn debug_only_types_through_the_marker() {
        assert_eq!(Debugged(vec![1, 2]).render(), "{[1, 2]}");
        assert_eq!(Debugged(Some("x")).render(), "{Some(\"x\")}");
        assert_eq!(Debugged((1, 'c')).render(), "{(1, 'c')}");
    }

    #[test]
    fn marker_is_a_distinct_type() {
        #[derive(Debug)]
        #[allow(dead_code)]
        struct Point(i32, i32);
        assert_eq!(Debugged(Point(1, 2)).render(), "{Point(1, 2)}", "тип без Display рендерится только через Debugged");
    }
}
