use std::fs;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");

    let text = fs::read_to_string("data/items.txt").expect("нет data/items.txt");
    let mut code = String::from("pub const ITEMS: &[(&str, u32)] = &[\n");
    for line in text.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        let (name, price) = line.split_once(',').expect("строка должна быть вида `имя,цена`");
        code.push_str(&format!("    ({:?}, {}),\n", name.trim(), price.trim()));
    }
    code.push_str("];\n");
    fs::write("src/generated.rs", code).expect("не удалось записать src/generated.rs");
}
