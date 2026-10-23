mod delicious_snacks {
    // Добавили `pub` и указали после `as` ожидаемый псевдоним.
    pub use self::fruits::PEAR as FRUIT;
    pub use self::veggies::CUCUMBER as VEGGIE;

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
