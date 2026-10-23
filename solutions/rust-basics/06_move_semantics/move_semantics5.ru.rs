#![allow(clippy::ptr_arg)]

// Заимствует вместо того, чтобы забирать владение.
// Здесь рекомендуется использовать `&str` вместо `&String`. Но пока
// хватит и этого, потому что со строками мы ещё не разбирались.
fn get_char(data: &String) -> char {
    data.chars().last().unwrap()
}

// Забирает владение вместо заимствования.
fn string_uppercase(mut data: String) {
    data = data.to_uppercase();

    println!("{data}");
}

fn main() {
    let data = "Rust is great!".to_string();

    get_char(&data);

    string_uppercase(data);
}
