// Пути к модулям можно подключать в область видимости и давать им новые имена
// с помощью ключевых слов `use` и `as`.

mod delicious_snacks {
    // TODO: Исправь и добавь следующие две инструкции `use`.
    // use self::fruits::PEAR as ???;
    // use self::veggies::CUCUMBER as ???;

    mod fruits {
        pub const PEAR: &str = "Pear";
        pub const APPLE: &str = "Apple";
    }

    mod veggies {
        pub const CUCUMBER: &str = "Cucumber";
        pub const CARROT: &str = "Carrot";
    }
}

fn main() {
    println!(
        "любимые закуски: {} и {}",
        delicious_snacks::FRUIT,
        delicious_snacks::VEGGIE,
    );
}
