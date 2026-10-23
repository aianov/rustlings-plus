// TODO: Исправь ошибку компилятора в функции `main`, не меняя эту функцию.
fn is_a_color_word(attempt: &str) -> bool {
    attempt == "green" || attempt == "blue" || attempt == "red"
}

fn main() {
    let word = String::from("green"); // Эту строку не меняй.

    if is_a_color_word(word) {
        println!("Это слово, обозначающее цвет, мне знакомо!");
    } else {
        println!("Это слово, обозначающее цвет, мне незнакомо.");
    }
}
