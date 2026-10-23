#![deny(clippy::use_self)] // потренируйся с типом `Self`

#[derive(Debug)]
struct Fireworks {
    rockets: usize,
}

impl Fireworks {
    fn new() -> Self {
        Self { rockets: 0 }
    }

    fn add_rockets(&mut self, rockets: usize) {
        self.rockets += rockets
    }

    fn start(self) -> String {
        "🚀".repeat(self.rockets)
    }
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
