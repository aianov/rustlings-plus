fn is_a_color_word(attempt: &str) -> bool {
    attempt == "green" || attempt == "blue" || attempt == "red"
}

fn main() {
    let word = String::from("green");

    if is_a_color_word(&word) {
        //             ^ добавлено, чтобы получить `&String`, который компилятор автоматически
        //               приводит к `&str`.
        println!("Это слово, обозначающее цвет, мне знакомо!");
    } else {
        println!("Это слово, обозначающее цвет, мне незнакомо.");
    }
}
