use std::path::PathBuf;
use std::process::Command;

fn tree(kind: &str) -> Vec<String> {
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["tree", "--edges", kind, "--depth", "1", "--prefix", "none", "--format", "{p}"])
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .output()
        .expect("не удалось запустить cargo tree");
    assert!(
        out.status.success(),
        "cargo tree не отработал:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
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
fn runtime_dependencies_are_only_fmtkit() {
    assert_eq!(
        tree("normal"),
        ["fmtkit"],
        "в [dependencies] должен остаться только fmtkit: он нужен коду библиотеки в готовой программе, остальное лишний вес для всех, кто подключит пакет"
    );
}

#[test]
fn build_dependencies_are_only_stamp() {
    assert_eq!(
        tree("build"),
        ["stamp"],
        "build.rs собирается отдельно и видит только [build-dependencies]: stamp должен лежать там"
    );
}

#[test]
fn dev_dependencies_are_only_testkit() {
    assert_eq!(
        tree("dev"),
        ["testkit"],
        "testkit нужен только тестам, его место в [dev-dependencies]: иначе он поедет к каждому, кто подключит пакет"
    );
}

#[test]
fn binary_prints_version_and_table() {
    let Some(exe) = option_env!("CARGO_BIN_EXE_report-tool") else {
        panic!("бинарник report-tool не собран");
    };
    let out = Command::new(exe).output().unwrap();
    assert!(out.status.success(), "report-tool завершился с ошибкой");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "report-tool v0.2.0\nusers | 42\nposts | 1337\n",
        "первая строка это версия из build.rs, дальше таблица из fmtkit"
    );
}

#[test]
fn library_renders_a_table() {
    assert_eq!(
        report_tool::render(&[("a", 1), ("bb", 22)]),
        "a  | 1\nbb | 22",
        "колонка имён должна выравниваться по самому длинному имени"
    );
}
