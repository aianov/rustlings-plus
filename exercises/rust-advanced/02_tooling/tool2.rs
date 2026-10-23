// Скрипты разработчика постоянно ходят по папкам: ищут корень воркспейса, перечисляют крейты, создают
// каталоги, склеивают пути из пользовательского ввода. Всё это есть в std::fs и std::path.
// Вот четыре функции из такого скрипта. Описания в комментариях перед ними.
// TODO: Реализуй их. Особая осторожность нужна с путями, которые приходят от пользователя:
// склеивать их с базовой папкой надо так, чтобы они не могли выбраться за её пределы.

use std::io;
use std::path::{Path, PathBuf};

/// Поднимается от `start` (файл или папка) вверх и возвращает первую папку, в чьём Cargo.toml
/// есть строка `[workspace]`. Если такой нет, возвращает None.
pub fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    todo!()
}

/// Имена папок внутри `root/crates`, в которых лежит Cargo.toml, по алфавиту.
/// Обычные файлы и папки без Cargo.toml пропускаются. Если `crates` нет, это ошибка ввода-вывода.
pub fn crate_names(root: &Path) -> io::Result<Vec<String>> {
    todo!()
}

/// Создаёт папку со всеми родителями. Возвращает true, если папки не было, и false, если она уже была.
pub fn ensure_dir(path: &Path) -> io::Result<bool> {
    todo!()
}

/// Склеивает `base` и пользовательский путь `rel`. Если `rel` абсолютный или пытается выйти вверх
/// через `..`, возвращает None.
pub fn resolve_inside(base: &Path, rel: &str) -> Option<PathBuf> {
    todo!()
}

fn main() {
    let here = std::env::current_dir().unwrap_or_default();
    println!("{:?}", find_workspace_root(&here));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tool2_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("не удалось создать временную папку");
        dir
    }

    fn make_crate(root: &Path, name: &str) {
        let dir = root.join("crates").join(name);
        fs::create_dir_all(&dir).expect("не удалось создать папку крейта");
        fs::write(dir.join("Cargo.toml"), "[package]\nname = \"x\"\n").expect("не удалось записать манифест");
    }

    #[test]
    fn root_is_found_from_a_deep_folder_and_from_a_file() {
        let root = tmp("root");
        fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = [\"crates/*\"]\n").expect("запись");
        make_crate(&root, "core");
        let deep = root.join("crates/core");
        assert_eq!(find_workspace_root(&deep), Some(root.clone()), "из папки крейта корень находится двумя уровнями выше");
        let file = deep.join("Cargo.toml");
        assert_eq!(find_workspace_root(&file), Some(root.clone()), "стартовать можно и от файла");
        assert_eq!(find_workspace_root(&root), Some(root), "корень находит сам себя");
    }

    #[test]
    fn plain_package_manifests_are_not_roots() {
        let outer = tmp("outer");
        fs::write(outer.join("Cargo.toml"), "[workspace]\n").expect("запись");
        let inner = outer.join("tools/one");
        fs::create_dir_all(&inner).expect("папка");
        fs::write(inner.join("Cargo.toml"), "[package]\nname = \"one\"\n").expect("запись");
        assert_eq!(find_workspace_root(&inner), Some(outer), "манифест без [workspace] не корень, надо идти выше");
    }

    #[test]
    fn no_root_means_none() {
        let lonely = tmp("lonely");
        let found = find_workspace_root(&lonely);
        assert!(
            found.as_ref().is_none_or(|p| !p.starts_with(&lonely)),
            "внутри пустой папки корня воркспейса нет: {found:?}"
        );
    }

    #[test]
    fn crate_names_are_sorted_and_filtered() {
        let root = tmp("names");
        make_crate(&root, "web");
        make_crate(&root, "auth");
        fs::create_dir_all(root.join("crates/empty")).expect("папка");
        fs::write(root.join("crates/README.md"), "notes").expect("запись");
        assert_eq!(
            crate_names(&root).expect("crates существует"),
            ["auth", "web"],
            "нужны только папки с Cargo.toml, по алфавиту"
        );
    }

    #[test]
    fn missing_crates_folder_is_an_error() {
        let root = tmp("nocrates");
        assert!(crate_names(&root).is_err(), "если папки crates нет, это ошибка, а не пустой список");
    }

    #[test]
    fn ensure_dir_reports_whether_it_created_something() {
        let root = tmp("ensure");
        let deep = root.join("a/b/c");
        assert!(ensure_dir(&deep).expect("создание"), "первый вызов создаёт папку");
        assert!(deep.is_dir());
        assert!(!ensure_dir(&deep).expect("повтор"), "повторный вызов ничего не создаёт");
    }

    #[test]
    fn resolve_inside_joins_honest_paths() {
        let base = Path::new("/srv/app");
        assert_eq!(resolve_inside(base, "logs/today.txt"), Some(PathBuf::from("/srv/app/logs/today.txt")));
        assert_eq!(
            resolve_inside(base, "./cfg.toml").map(|p| p.components().collect::<PathBuf>()),
            Some(PathBuf::from("/srv/app/cfg.toml")),
            "точка в начале пути безобидна"
        );
    }

    #[test]
    fn resolve_inside_refuses_escapes() {
        let base = Path::new("/srv/app");
        assert_eq!(resolve_inside(base, "/etc/passwd"), None, "абсолютный путь при join заменил бы базу целиком");
        assert_eq!(resolve_inside(base, "../secrets"), None, "выход вверх через ..");
        assert_eq!(resolve_inside(base, "logs/../../x"), None, ".. посреди пути тоже опасен");
    }
}
