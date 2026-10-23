use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("не удалось создать папку");
    for entry in fs::read_dir(from).expect("не удалось прочитать папку") {
        let entry = entry.expect("ошибка чтения папки");
        let name = entry.file_name();
        if name == "target" || name == "generated.rs" {
            continue;
        }
        let (src, dst) = (entry.path(), to.join(&name));
        if src.is_dir() {
            copy_tree(&src, &dst);
        } else {
            fs::copy(&src, &dst).expect("не удалось скопировать файл");
        }
    }
}

struct Sandbox {
    dir: PathBuf,
    target: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Sandbox {
        let base = Path::new(env!("CARGO_TARGET_TMPDIR"));
        let dir = base.join(format!("sandbox_{name}"));
        let target = base.join(format!("sandbox_target_{name}"));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&target);
        copy_tree(&root(), &dir);
        Sandbox { dir, target }
    }

    fn cargo(&self, args: &[&str]) -> Output {
        Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .args(args)
            .arg("--target-dir")
            .arg(&self.target)
            .current_dir(&self.dir)
            .output()
            .expect("не удалось запустить cargo")
    }

    fn build(&self) -> String {
        let out = self.cargo(&["build", "-v"]);
        let log = String::from_utf8_lossy(&out.stderr).to_string();
        assert!(out.status.success(), "сборка упала:\n{log}");
        log
    }

    fn items(&self) -> String {
        let out = self.cargo(&["run", "--quiet"]);
        assert!(out.status.success(), "запуск упал:\n{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }
}

#[test]
fn generated_code_stays_out_of_src() {
    let sb = Sandbox::new("src");
    sb.build();
    assert!(
        !sb.dir.join("src/generated.rs").exists(),
        "build.rs записал файл в src/: сгенерированное место в OUT_DIR, иначе каждая сборка сама меняет пакет"
    );
}

#[test]
fn second_build_without_changes_is_fresh() {
    let sb = Sandbox::new("fresh");
    sb.build();
    let log = sb.build();
    assert!(
        log.contains("Fresh menu-gen") && !log.contains("Compiling menu-gen"),
        "повторная сборка без изменений должна быть Fresh, а скрипт сборки не должен запускаться заново:\n{log}"
    );
}

#[test]
fn editing_the_data_file_rebuilds_the_table() {
    let sb = Sandbox::new("data");
    assert_eq!(sb.items(), "items: 3", "в меню три позиции");
    std::thread::sleep(Duration::from_millis(1200));
    let path = sb.dir.join("data/items.txt");
    let mut text = fs::read_to_string(&path).expect("нет data/items.txt");
    text.push_str("mocha,400\n");
    fs::write(&path, text).expect("не удалось записать data/items.txt");
    assert_eq!(
        sb.items(),
        "items: 4",
        "после правки data/items.txt таблица должна обновиться: проверь rerun-if-changed"
    );
}

#[test]
fn table_in_the_project_is_correct() {
    assert_eq!(menu_gen::ITEMS.len(), 3, "в data/items.txt три позиции");
    assert_eq!(menu_gen::ITEMS[1], ("latte", 350), "вторая позиция это latte за 350");
}
