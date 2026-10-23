fn main() {
    let my_first_initial = 'C';
    if my_first_initial.is_alphabetic() {
        println!("Буква!");
    } else if my_first_initial.is_numeric() {
        println!("Цифра!");
    } else {
        println!("Ни буква, ни цифра!");
    }

    // Пример с эмодзи.
    let your_character = '🦀';

    if your_character.is_alphabetic() {
        println!("Буква!");
    } else if your_character.is_numeric() {
        println!("Цифра!");
    } else {
        println!("Ни буква, ни цифра!");
    }
}
