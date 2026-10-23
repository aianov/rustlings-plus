// Вложения чата сохраняются под именем, которое прислал клиент: `root/<имя>`. Один умелец прислал файл с именем
// `../../etc/cron.d/job`, другой абсолютный путь, третий `....//....//x`, и файлы появились за пределами хранилища.
// Имя от клиента это просто строка для показа, а не путь. Цель: `save_upload` кладёт файл только внутрь `root` под ключом,
// который выбирает сервер (в подкаталог из первых двух символов, как раньше), а клиентское имя возвращает как
// `display_name` для показа: последняя часть после `/` или `\`, без управляющих символов, без точек в начале, не
// длиннее 100 символов; если ничего не осталось, то `file`.

use std::io;
use std::path::{Path, PathBuf};

fn main() {}

#[derive(Debug, PartialEq)]
pub struct Saved {
    pub key: String,
    pub display_name: String,
}

pub fn save_upload(root: &Path, client_name: &str, data: &[u8], id: u64) -> io::Result<Saved> {
    let _ = id;
    let path: PathBuf = root.join(client_name);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, data)?;
    Ok(Saved { key: client_name.to_string(), display_name: client_name.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> TestDir {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let path = std::env::temp_dir().join(format!("svfile9_{}_{}_{}", std::process::id(), nanos, COUNTER.fetch_add(1, Ordering::SeqCst)));
            std::fs::create_dir_all(path.join("deep").join("store")).unwrap();
            TestDir(path)
        }

        fn root(&self) -> PathBuf {
            self.0.join("deep").join("store")
        }

        fn files(&self) -> Vec<PathBuf> {
            let mut found = Vec::new();
            let mut stack = vec![self.0.clone()];
            while let Some(dir) = stack.pop() {
                for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                    let path = entry.path();
                    if path.is_dir() { stack.push(path) } else { found.push(path) }
                }
            }
            found.sort();
            found
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn hostile_names_never_escape_the_store() {
        let dir = TestDir::new();
        let absolute = format!("{}/escaped_absolute.txt", dir.0.display());
        let names = [
            "../../outside.txt".to_string(),
            "../outside.txt".to_string(),
            absolute,
            "....//....//deep.txt".to_string(),
            "a/../../b.txt".to_string(),
            "..\\..\\win.txt".to_string(),
            "C:\\Users\\x\\cat.jpg".to_string(),
            "/".to_string(),
            "..".to_string(),
            "".to_string(),
        ];
        for (i, name) in names.iter().enumerate() {
            save_upload(&dir.root(), name, b"payload", i as u64 + 1).unwrap_or_else(|e| panic!("имя {name:?} не должно приводить к ошибке: {e}"));
        }
        let outside: Vec<PathBuf> = dir.files().into_iter().filter(|f| !f.starts_with(dir.root())).collect();
        assert!(outside.is_empty(), "файлы за пределами хранилища: {outside:?}");
        assert_eq!(dir.files().len(), names.len(), "каждый загруженный файл лежит внутри хранилища");
    }

    #[test]
    fn the_server_picks_the_key_and_the_place() {
        let dir = TestDir::new();
        let saved = save_upload(&dir.root(), "../../x.png", b"data", 0xAB12).unwrap();
        assert!(!saved.key.contains(['/', '\\', '.']), "ключ не содержит разделителей и точек: {:?}", saved.key);
        assert!(saved.key.len() >= 4 && saved.key.chars().all(|c| c.is_ascii_alphanumeric()), "ключ из букв и цифр: {:?}", saved.key);
        let path = dir.root().join(&saved.key[..2]).join(&saved.key);
        assert_eq!(std::fs::read(path).expect("файл лежит root/<первые два символа>/<ключ>"), b"data", "содержимое на месте");
    }

    #[test]
    fn two_uploads_with_the_same_name_do_not_overwrite_each_other() {
        let dir = TestDir::new();
        let a = save_upload(&dir.root(), "photo.png", b"first", 1).unwrap();
        let b = save_upload(&dir.root(), "photo.png", b"second", 2).unwrap();
        assert_ne!(a.key, b.key, "разные загрузки получают разные ключи, хоть имя и совпало");
        assert_eq!(dir.files().len(), 2, "оба файла лежат");
    }

    #[test]
    fn the_display_name_is_safe_to_show() {
        let dir = TestDir::new();
        let show = |name: &str| save_upload(&dir.root(), name, b"x", 1).unwrap().display_name;
        assert_eq!(show("holiday.png"), "holiday.png", "обычное имя не меняется");
        assert_eq!(show("../../outside.txt"), "outside.txt", "берётся последняя часть пути");
        assert_eq!(show("C:\\Users\\x\\cat.jpg"), "cat.jpg", "обратная косая черта тоже разделитель");
        assert_eq!(show("a/b/"), "file", "после разделителя пусто, остаётся запасное имя");
        assert_eq!(show(".."), "file", "две точки не имя");
        assert_eq!(show(""), "file", "пустое имя");
        assert_eq!(show(".hidden"), "hidden", "точки в начале убраны");
        assert_eq!(show("na\0me\n.png"), "name.png", "управляющие символы убраны");
        assert_eq!(show("фото 1.png"), "фото 1.png", "не ASCII-имена остаются");
        assert_eq!(show(&"я".repeat(300)).chars().count(), 100, "не длиннее ста символов");
    }
}
