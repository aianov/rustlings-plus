// Вызовы этой функции нужно заменить вызовами `string_slice` или `string`.
fn placeholder() {}

fn string_slice(arg: &str) {
    println!("{arg}");
}

fn string(arg: String) {
    println!("{arg}");
}

// TODO: Вот набор значений: какие-то это `String`, какие-то `&str`.
// Твоя задача: заменить `placeholder(…)` на `string_slice(…)`
// или `string(…)`, в зависимости от того, чем, по-твоему, является каждое значение.
fn main() {
    placeholder("blue");

    placeholder("red".to_string());

    placeholder(String::from("hi"));

    placeholder("rust is fun!".to_owned());

    placeholder(format!("Interpolation {}", "Station"));

    // ВНИМАНИЕ: это индексация по байтам, а не по символам.
    // Индексацию по символам можно сделать через `s.chars().nth(INDEX)`.
    placeholder(&String::from("abc")[0..1]);

    placeholder("  hello there ".trim());

    placeholder("Happy Monday!".replace("Mon", "Tues"));

    placeholder("mY sHiFt KeY iS sTiCkY".to_lowercase());
}
