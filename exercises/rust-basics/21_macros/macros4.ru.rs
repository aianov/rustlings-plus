// TODO: Исправь ошибку компилятора, добавив один или два символа.
#[rustfmt::skip]
macro_rules! my_macro {
    () => {
        println!("Зацени мой макрос!");
    }
    ($val:expr) => {
        println!("Смотри, другой макрос: {}", $val);
    }
}

fn main() {
    my_macro!();
    my_macro!(7777);
}
