use std::path::PathBuf;

pub const UPDATE_ENV: &str = "UPDATE_GOLDEN";

pub const GOLDEN_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/golden");

fn updating() -> bool {
    std::env::var(UPDATE_ENV).is_ok_and(|value| value == "1")
}

pub fn golden(name: &str, actual: &str) {
    let path = PathBuf::from(GOLDEN_DIR).join(name);
    let actual = format!("{}\n", actual.trim_end());
    if updating() {
        std::fs::create_dir_all(GOLDEN_DIR).expect("папка golden создаётся");
        std::fs::write(&path, &actual).expect("золотой файл пишется");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "нет золотого файла {} ({error}): запусти с {UPDATE_ENV}=1 и посмотри diff",
            path.display()
        )
    });
    if expected != actual {
        let first = expected
            .lines()
            .zip(actual.lines())
            .position(|(left, right)| left != right)
            .unwrap_or_else(|| expected.lines().count().min(actual.lines().count()));
        panic!(
            "формат {name} разошёлся с золотым файлом со строки {}:\nбыло:\n{expected}\nстало:\n{actual}\nесли так и задумано, запусти с {UPDATE_ENV}=1",
            first + 1
        );
    }
}
