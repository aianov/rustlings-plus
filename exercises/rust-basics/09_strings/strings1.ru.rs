// TODO: Исправь ошибку компилятора, не меняя сигнатуру функции.
fn current_favorite_color() -> String {
    "blue"
}

fn main() {
    let answer = current_favorite_color();
    println!("Мой нынешний любимый цвет: {answer}");
}
