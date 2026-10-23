fn current_favorite_color() -> String {
    // Равносильно `String::from("blue")`
    "blue".to_string()
}

fn main() {
    let answer = current_favorite_color();
    println!("Мой нынешний любимый цвет: {answer}");
}
