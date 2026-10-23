use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn tmp(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(name)
}

fn cargo_bin() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".into())
}

fn run_with_stdin(mut cmd: Command, input: &str) -> Output {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("не удалось запустить программу");
    child
        .stdin
        .take()
        .expect("нет stdin")
        .write_all(input.as_bytes())
        .expect("не удалось записать в stdin");
    child.wait_with_output().expect("не удалось дождаться программы")
}

fn exe(name: &str) -> Command {
    let path = match name {
        "textstat" => option_env!("CARGO_BIN_EXE_textstat"),
        "textstat-words" => option_env!("CARGO_BIN_EXE_textstat-words"),
        _ => None,
    };
    match path {
        Some(p) => Command::new(p),
        None => panic!("бинарник {name} не собран: проверь файлы в src и src/bin"),
    }
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("не удалось создать папку");
    for entry in fs::read_dir(from).expect("не удалось прочитать папку") {
        let entry = entry.expect("ошибка чтения папки");
        let name = entry.file_name();
        if name == "target" {
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

fn sandbox(name: &str) -> PathBuf {
    let dir = tmp(&format!("box_{name}"));
    let _ = fs::remove_dir_all(&dir);
    copy_tree(&root(), &dir);
    dir
}

fn cargo_in(dir: &Path, target: &str, args: &[&str]) -> Output {
    Command::new(cargo_bin())
        .args(args)
        .arg("--target-dir")
        .arg(tmp(target))
        .current_dir(dir)
        .output()
        .expect("не удалось запустить cargo")
}

fn tree(kind: &str) -> Vec<String> {
    let out = Command::new(cargo_bin())
        .args(["tree", "--edges", kind, "--depth", "1", "--prefix", "none", "--format", "{p}"])
        .current_dir(root())
        .output()
        .expect("не удалось запустить cargo tree");
    assert!(out.status.success(), "cargo tree не отработал:\n{}", String::from_utf8_lossy(&out.stderr));
    let mut names: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .skip(1)
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_string)
        .collect();
    names.sort();
    names
}

#[test]
fn manifest_declares_edition_and_rust_version() {
    let out = Command::new(cargo_bin())
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root())
        .output()
        .expect("не удалось запустить cargo metadata");
    let meta = String::from_utf8_lossy(&out.stdout);
    assert!(meta.contains("\"edition\":\"2024\""), "нужен edition 2024");
    assert!(meta.contains("\"rust_version\":\"1.88\""), "нужна rust-version = \"1.88\"");
    assert!(
        meta.contains("\"kind\":[\"bin\"],\"crate_types\":[\"bin\"],\"name\":\"textstat-words\""),
        "нужен второй бинарник textstat-words"
    );
}

#[test]
fn dependency_kinds_are_right() {
    assert_eq!(tree("normal"), ["tokenizer"], "tokenizer нужен коду программы: [dependencies]");
    assert_eq!(tree("dev"), ["samples"], "samples нужен только тестам: [dev-dependencies]");
    assert!(tree("build").is_empty(), "build.rs обходится стандартной библиотекой: [build-dependencies] не нужны");
}

#[test]
fn library_analyzes_text() {
    let report = textstat::analyze(samples::TEXT);
    assert_eq!(report, textstat::Report { lines: 2, words: 10, chars: 41 }, "строки, слова и символы примера");
    assert_eq!(
        textstat::render_text(&report),
        "lines: 2\nwords: 10\nchars: 41",
        "render_text печатает три строки"
    );
}

#[test]
fn top_words_skip_stopwords() {
    assert_eq!(
        textstat::top_words(samples::TEXT, 2),
        [("cat".to_string(), 2), ("bird".to_string(), 1)],
        "the и and это стоп-слова; при равной частоте слова идут по алфавиту"
    );
    assert_eq!(textstat::top_words(samples::TEXT, 10).len(), 3, "значимых слов всего три: cat, dog, bird");
}

#[test]
fn stopwords_come_from_the_data_file() {
    assert_eq!(
        textstat::stopwords(),
        ["a", "an", "and", "in", "of", "the", "to"],
        "стоп-слова: без комментариев и повторов, по алфавиту"
    );
    assert_eq!(textstat::VERSION, "textstat 0.4.0 (7 stopwords)", "VERSION собирает build.rs");
}

#[test]
fn binary_prints_report_top_and_version() {
    let out = run_with_stdin(exe("textstat"), samples::TEXT);
    assert!(out.status.success(), "textstat упал: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "lines: 2\nwords: 10\nchars: 41", "отчёт по умолчанию");

    let mut cmd = exe("textstat");
    cmd.args(["--top", "2"]);
    let out = run_with_stdin(cmd, samples::TEXT);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "cat 2\nbird 1", "--top 2 печатает `слово число`");

    let mut cmd = exe("textstat");
    cmd.arg("--version");
    let out = run_with_stdin(cmd, "");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "textstat 0.4.0 (7 stopwords)", "--version");
}

#[test]
fn second_binary_lists_stopwords() {
    let out = exe("textstat-words").output().expect("не удалось запустить textstat-words");
    assert!(out.status.success(), "textstat-words упал");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "a\nan\nand\nin\nof\nthe\nto\n",
        "textstat-words печатает по слову на строке"
    );
}

#[test]
fn json_output_exists_only_with_the_feature() {
    let mut cmd = Command::new(cargo_bin());
    cmd.args(["run", "--quiet", "--bin", "textstat", "--features", "json", "--target-dir"])
        .arg(tmp("toolbox_json"))
        .args(["--", "--json"])
        .current_dir(root());
    let out = run_with_stdin(cmd, samples::TEXT);
    assert!(out.status.success(), "с feature json --json должен работать: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "{\"lines\":2,\"words\":10,\"chars\":41}",
        "формат JSON"
    );

    let mut cmd = exe("textstat");
    cmd.arg("--json");
    let out = run_with_stdin(cmd, samples::TEXT);
    assert_eq!(out.status.code(), Some(2), "без feature json флаг --json должен завершаться с кодом 2");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("json"),
        "без feature json в stderr должно быть объяснение со словом json"
    );
}

#[test]
fn generated_code_stays_out_of_src() {
    let mut files: Vec<String> = fs::read_dir(root().join("src"))
        .expect("нет папки src")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    files.sort();
    assert!(
        !files.iter().any(|f| f.contains("stopwords")),
        "в src лежит сгенерированный файл: build.rs должен писать в OUT_DIR"
    );
}

#[test]
fn release_profile_is_tuned() {
    let dir = tmp("toolbox_release");
    let _ = fs::remove_dir_all(&dir);
    let out = Command::new(cargo_bin())
        .args(["build", "--release", "-v", "--bin", "textstat", "--target-dir"])
        .arg(&dir)
        .current_dir(root())
        .output()
        .expect("не удалось запустить cargo");
    let log = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "release-сборка упала:\n{log}");
    let line = log
        .lines()
        .find(|l| l.contains("--crate-name textstat ") && l.contains("--crate-type bin"))
        .unwrap_or_else(|| panic!("нет запуска rustc для бинарника textstat:\n{log}"));
    assert!(line.contains("-C lto"), "в [profile.release] нужна lto = true");
    assert!(line.contains("-C codegen-units=1"), "в [profile.release] нужна codegen-units = 1");
}

#[test]
fn lints_are_enforced() {
    for (name, code, needle, advice) in [
        ("unwrap", "\npub fn later(x: Option<u8>) -> u8 {\n    x.unwrap()\n}\n", "unwrap_used", "clippy::unwrap_used = \"deny\""),
        ("unsafe", "\npub fn later() -> u8 {\n    unsafe { 1 }\n}\n", "unsafe", "unsafe_code = \"forbid\""),
    ] {
        let dir = sandbox(&format!("lint_{name}"));
        let lib = dir.join("src/lib.rs");
        let mut text = fs::read_to_string(&lib).expect("нет src/lib.rs");
        text.push_str(code);
        fs::write(&lib, text).expect("не удалось записать src/lib.rs");
        let out = cargo_in(&dir, &format!("toolbox_lint_{name}"), &["clippy", "--quiet", "--lib"]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success() && stderr.contains(needle),
            "в [lints] должно быть {advice}. Вывод:\n{stderr}"
        );
    }
}

#[test]
fn editing_stopwords_updates_the_build_string() {
    let dir = sandbox("rerun");
    let run = || {
        let mut cmd = Command::new(cargo_bin());
        cmd.args(["run", "--quiet", "--bin", "textstat", "--target-dir"])
            .arg(tmp("toolbox_rerun"))
            .args(["--", "--version"])
            .current_dir(&dir);
        let out = run_with_stdin(cmd, "");
        assert!(out.status.success(), "сборка в копии упала: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    assert_eq!(run(), "textstat 0.4.0 (7 stopwords)", "до правки в словаре семь слов");
    std::thread::sleep(Duration::from_millis(1200));
    let path = dir.join("data/stopwords.txt");
    let mut text = fs::read_to_string(&path).expect("нет data/stopwords.txt");
    text.push_str("is\n");
    fs::write(&path, text).expect("не удалось записать data/stopwords.txt");
    assert_eq!(
        run(),
        "textstat 0.4.0 (8 stopwords)",
        "после правки словаря строка версии должна обновиться: проверь rerun-if-changed"
    );
}
