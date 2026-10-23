// Добавлены точки с запятой, разделяющие ветки макроса.
#[rustfmt::skip]
macro_rules! my_macro {
    () => {
        println!("Зацени мой макрос!");
    };
    ($val:expr) => {
        println!("Смотри, другой макрос: {}", $val);
    };
}

fn main() {
    my_macro!();
    my_macro!(7777);
}
