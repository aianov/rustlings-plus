// Структуры хранят данные, но могут содержать и логику. В этом упражнении мы
// определили структуру `Fireworks` и пару функций, которые с ней работают.
// Преврати эти самостоятельные функции в методы и ассоциированные функции,
// чтобы эта связь была яснее видна в коде.

#![deny(clippy::use_self)] // потренируйся с типом `Self`

#[derive(Debug)]
struct Fireworks {
    rockets: usize,
}

// TODO: Преврати эту функцию в ассоциированную функцию `Fireworks`.
fn new_fireworks() -> Fireworks {
    Fireworks { rockets: 0 }
}

// TODO: Преврати эту функцию в метод `Fireworks`.
fn add_rockets(fireworks: &mut Fireworks, rockets: usize) {
    fireworks.rockets += rockets
}

// TODO: Преврати эту функцию в метод `Fireworks`.
fn start(fireworks: Fireworks) -> String {
    "🚀".repeat(fireworks.rockets)
}

fn main() {
    // Здесь можно по желанию поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_some_fireworks() {
        let f = Fireworks::new();
        assert_eq!(f.start(), "");

        let mut f = Fireworks::new();
        f.add_rockets(3);
        assert_eq!(f.start(), "🚀🚀🚀");

        let mut f = Fireworks::new();
        f.add_rockets(7);
        // В последнем тесте мы не используем синтаксис метода, чтобы убедиться, что
        // функция `start` забирает владение фейерверком.
        assert_eq!(Fireworks::start(f), "🚀🚀🚀🚀🚀🚀🚀");
    }
}
