use std::fs;
use std::io;
use std::path::Path;

const MASK: &str = "\"src/microservices/*\"";

pub fn create(root: &Path, name: &str) -> Result<(), String> {
    check_name(name)?;
    let dir = root.join("src").join("microservices").join(name);
    if dir.exists() {
        return Err(format!("src/microservices/{name} уже существует"));
    }
    let manifest_path = root.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("не прочитать Cargo.toml корня: {e}"))?;
    let registered = with_mask(&manifest)?;
    let fail = |e: io::Error| format!("src/microservices/{name}: {e}");
    fs::create_dir_all(dir.join("src")).map_err(fail)?;
    fs::write(dir.join("Cargo.toml"), service_manifest(name)).map_err(fail)?;
    fs::write(dir.join("src").join("main.rs"), service_main(name)).map_err(fail)?;
    if registered != manifest {
        fs::write(&manifest_path, registered)
            .map_err(|e| format!("не записать Cargo.toml корня: {e}"))?;
    }
    println!("создан src/microservices/{name}");
    Ok(())
}

fn check_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let starts_with_letter = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    let allowed = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if starts_with_letter && allowed && !name.ends_with('-') && !name.contains("--") {
        Ok(())
    } else {
        Err(format!(
            "плохое имя `{name}`: маленькие латинские буквы, цифры и одиночные дефисы, \
             начинается с буквы и не кончается дефисом"
        ))
    }
}

fn with_mask(manifest: &str) -> Result<String, String> {
    if manifest.contains(MASK) {
        return Ok(manifest.to_string());
    }
    let mut lines = Vec::new();
    let mut found = false;
    for line in manifest.lines() {
        let items = line
            .strip_prefix("members = [")
            .and_then(|rest| rest.strip_suffix(']'));
        match items {
            Some(items) if !found => {
                let mut members: Vec<&str> = items
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .collect();
                members.push(MASK);
                members.sort_unstable();
                lines.push(format!("members = [{}]", members.join(", ")));
                found = true;
            }
            _ => lines.push(line.to_string()),
        }
    }
    if found {
        Ok(lines.join("\n") + "\n")
    } else {
        Err(format!(
            "в Cargo.toml корня нет строки `members = [...]` в одну строку: добавь {MASK} сам"
        ))
    }
}

fn service_manifest(name: &str) -> String {
    format!(
        "[package]\nname = \"{name}\"\nversion.workspace = true\nedition.workspace = true\n\
         rust-version.workspace = true\npublish.workspace = true\n\n[dependencies]\n\n\
         [lints]\nworkspace = true\n"
    )
}

fn service_main(name: &str) -> String {
    format!("fn main() {{\n    println!(\"{name} started\");\n}}\n")
}
