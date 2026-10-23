#![allow(clippy::ptr_arg)]

// TODO: Исправь ошибки компилятора, ничего не меняя, кроме добавления или
// удаления ссылок (символа `&`).

// Не должна забирать владение
fn get_char(data: String) -> char {
    data.chars().last().unwrap()
}

// Должна забирать владение
fn string_uppercase(mut data: &String) {
    data = data.to_uppercase();

    println!("{data}");
}

fn main() {
    let data = "Rust is great!".to_string();

    get_char(data);

    string_uppercase(&data);
}
