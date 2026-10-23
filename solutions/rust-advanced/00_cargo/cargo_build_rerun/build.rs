use std::path::PathBuf;
use std::{env, fs};

fn main() {
    println!("cargo::rerun-if-changed=data/items.txt");

    let text = fs::read_to_string("data/items.txt").expect("нет data/items.txt");
    let mut code = String::from("pub const ITEMS: &[(&str, u32)] = &[\n");
    for line in text.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        let (name, price) = line.split_once(',').expect("строка должна быть вида `имя,цена`");
        code.push_str(&format!("    ({:?}, {}),\n", name.trim(), price.trim()));
    }
    code.push_str("];\n");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("cargo задаёт OUT_DIR"));
    fs::write(out_dir.join("items.rs"), code).expect("не удалось записать items.rs");
}
