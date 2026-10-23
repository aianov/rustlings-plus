// TODO: Исправь ошибку компилятора, не вынося определение макроса из этого
// модуля.
mod macros {
    macro_rules! my_macro {
        () => {
            println!("Зацени мой макрос!");
        };
    }
}

fn main() {
    my_macro!();
}
