use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    str::from_utf8,
};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixture")
}

fn rustlings(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rustlings"))
        .current_dir(dir)
        .args(args)
        .env("RUSTLINGS_CONTENT_DIR", fixture())
        .env("RUSTLINGS_PG_URL", "postgres://u:p@127.0.0.1:1/db")
        .env_remove("CARGO_TARGET_DIR")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[track_caller]
fn ok(dir: &Path, args: &[&str]) -> String {
    let out = rustlings(dir, args);
    let stdout = from_utf8(&out.stdout).unwrap().to_owned();
    assert!(
        out.status.success(),
        "{args:?} failed\nstdout:\n{stdout}\nstderr:\n{}",
        from_utf8(&out.stderr).unwrap()
    );
    stdout
}

#[track_caller]
fn fail(dir: &Path, args: &[&str]) -> (String, String) {
    let out = rustlings(dir, args);
    let stdout = from_utf8(&out.stdout).unwrap().to_owned();
    let stderr = from_utf8(&out.stderr).unwrap().to_owned();
    assert!(
        !out.status.success(),
        "{args:?} unexpectedly succeeded\nstdout:\n{stdout}"
    );
    (stdout, stderr)
}

fn read(dir: &Path, path: &str) -> String {
    fs::read_to_string(dir.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn write(dir: &Path, path: &str, content: &str) {
    let path = dir.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn init(lang: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::TempDir::new().unwrap();
    let (stdout, _) = fail(tmp.path(), &[]);
    assert!(stdout.contains("rustlings init"));
    ok(tmp.path(), &["init", "--lang", lang]);
    let root = tmp.path().join("rustlings");
    (tmp, root)
}

#[test]
fn init_creates_the_learner_folder() {
    let (tmp, root) = init("en");

    let (_, stderr) = fail(tmp.path(), &["init", "--lang", "en"]);
    assert!(stderr.contains("`cd rustlings`"));
    let (_, stderr) = fail(&root, &["init"]);
    assert!(stderr.contains("already initialized"));

    let manifest = read(&root, "Cargo.toml");
    assert!(manifest.contains("members = [\"tracks/*\"]"));
    assert!(
        manifest.contains("exclude = [\"exercises\", \"solutions\", \"support\", \"backend\"]")
    );
    let track = read(&root, "tracks/basics/Cargo.toml");
    assert!(track.contains("name = \"basics-exercises\""));
    assert!(track.contains("path = \"../../exercises/basics/00_intro/test_success.rs\""));
    assert!(!track.contains("proj_sum"));
    assert!(read(&root, ".rustlings-settings.toml").contains("locale = \"en\""));
    assert_eq!(
        read(&root, "exercises/basics/00_intro/README.md"),
        "# Intro\n\nEnglish theory.\n"
    );
    assert!(read(&root, "exercises/basics/00_intro/test_success.rs").starts_with("// Everything"));
    assert!(
        !root
            .join("exercises/basics/00_intro/test_success.ru.rs")
            .exists()
    );
    assert!(!root.join("exercises/basics/00_intro/chapter.toml").exists());
    assert!(
        root.join("exercises/basics/00_intro/visuals/flow.html")
            .is_file()
    );
    assert!(
        root.join("exercises/basics/01_proj/proj_sum/tests/acceptance.rs")
            .is_file()
    );
    assert!(
        root.join("exercises/basics/02_journey/ms_first/README.md")
            .is_file()
    );
    assert!(
        root.join("exercises/basics/02_journey/ms_first/arch.toml")
            .is_file()
    );
    assert!(
        !root
            .join("exercises/basics/02_journey/ms_first/inject")
            .exists()
    );
    assert!(
        !root
            .join("exercises/basics/02_journey/ms_first/scaffold")
            .exists()
    );
    assert!(root.join("support/Cargo.toml").is_file());
    assert!(root.join("docker-compose.yml").is_file());
    assert!(root.join("solutions/README.md").is_file());
    assert!(root.join("rust-analyzer.toml").is_file());
    assert!(
        read(&root, ".vscode/settings.json")
            .contains("exercises/basics/01_proj/proj_sum/Cargo.toml")
    );
}

#[test]
fn file_exercises() {
    let (_tmp, root) = init("en");

    ok(&root, &["run", "compilation_success"]);
    fail(&root, &["run", "compilation_failure"]);
    let stdout = ok(&root, &["run", "test_success"]);
    assert!(stdout.contains("\nOutput from `main` function\n"));
    assert!(read(&root, ".rustlings-state.txt").contains("\ntest_success"));
    assert!(
        root.join("solutions/basics/00_intro/test_success.rs")
            .is_file()
    );
    fail(&root, &["run", "test_failure"]);
    let (_, stderr) = fail(&root, &["run", "not_in_course"]);
    assert!(stderr.contains("not_in_course"));

    let stdout = ok(&root, &["hint", "test_failure"]);
    assert!(stdout.contains("The answer to everything: 42"));

    fs::remove_file(root.join("exercises/basics/00_intro/data.txt")).unwrap();
    let stdout = ok(&root, &["run", "reads_input"]);
    assert!(stdout.contains("data: input file content"));

    write(&root, "exercises/basics/00_intro/test_failure.rs", "broken");
    ok(&root, &["reset", "test_failure"]);
    assert!(read(&root, "exercises/basics/00_intro/test_failure.rs").contains("TODO"));
    let solution = read(&root, "exercises/basics/00_intro/test_failure.rs")
        .replace("    // TODO: return the answer.\n    0\n", "    42\n");
    write(
        &root,
        "exercises/basics/00_intro/test_failure.rs",
        &solution,
    );
    ok(&root, &["run", "test_failure"]);

    let (stdout, _) = fail(&root, &["run", "needs_pg"]);
    assert!(stdout.contains("docker compose up -d postgres"));
    assert!(root.join("target/debug").is_dir());
}

#[test]
fn navigation_commands() {
    let (_tmp, root) = init("en");
    let stdout = ok(&root, &["track"]);
    assert!(stdout.contains("basics") && stdout.contains("Basics"));
    let stdout = ok(&root, &["track", "basics"]);
    assert!(stdout.contains("compilation_success"));
    fail(&root, &["track", "nope"]);
    let stdout = ok(&root, &["open", "proj_sum"]);
    assert!(stdout.contains("exercises/basics/01_proj/proj_sum/README.md"));
    assert!(read(&root, ".rustlings-state.txt").contains("\n\nproj_sum\n"));
    let stdout = ok(&root, &["lang"]);
    assert!(stdout.contains("en"));
}

#[test]
fn language_switch_keeps_edits() {
    let (_tmp, root) = init("en");
    let path = "exercises/basics/00_intro/test_success.rs";
    let edited = read(&root, path).replace(
        "    println!(\"Output from `main` function\");\n",
        "    println!(\"Output from `main` function\");\n    let mine = 1;\n",
    );
    write(&root, path, &edited);

    let stdout = ok(&root, &["lang", "ru"]);
    assert!(stdout.contains("Русский"));
    let merged = read(&root, path);
    assert!(merged.starts_with("// Здесь уже всё работает.\n"));
    assert!(merged.contains("    // Выведи что-нибудь.\n"));
    assert!(merged.contains("    let mine = 1;\n"));
    assert_eq!(
        read(&root, "exercises/basics/00_intro/README.md"),
        "# Введение\n\nТеория по-русски.\n"
    );
    assert!(read(&root, "exercises/basics/01_proj/proj_sum/README.md").contains("Сделай"));
    assert_eq!(
        read(&root, "exercises/basics/00_intro/compilation_success.rs"),
        "fn main() {}\n"
    );
    assert!(read(&root, ".rustlings-settings.toml").contains("locale = \"ru\""));
    let stdout = ok(&root, &["hint", "test_success"]);
    assert!(stdout.contains("Русская подсказка"));

    ok(&root, &["lang", "en"]);
    assert_eq!(read(&root, path), edited);
}

#[test]
fn project_exercise() {
    let (_tmp, root) = init("en");
    let dir = "exercises/basics/01_proj/proj_sum";
    fail(&root, &["run", "proj_sum"]);

    write(
        &root,
        &format!("{dir}/src/lib.rs"),
        "pub fn sum(a: i32, b: i32) -> i32 {\n    a * b\n}\n",
    );
    write(
        &root,
        &format!("{dir}/tests/acceptance.rs"),
        "#[test]\nfn cheat() {}\n",
    );
    fail(&root, &["run", "proj_sum"]);
    assert!(read(&root, &format!("{dir}/tests/acceptance.rs")).contains("assert_eq!"));

    write(
        &root,
        &format!("{dir}/src/lib.rs"),
        "pub fn sum(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
    );
    ok(&root, &["run", "proj_sum"]);
    assert!(
        root.join("solutions/basics/01_proj/proj_sum/src/lib.rs")
            .is_file()
    );
    assert!(!root.join(format!("{dir}/target")).exists());
    assert!(root.join("target/debug").is_dir());

    ok(&root, &["reset", "proj_sum"]);
    assert!(read(&root, &format!("{dir}/src/lib.rs")).contains("0\n"));
}

#[test]
fn milestones() {
    let (_tmp, root) = init("en");

    ok(&root, &["open", "ms_first"]);
    assert!(root.join("backend/Cargo.toml").is_file());
    assert!(read(&root, ".vscode/settings.json").contains("backend/Cargo.toml"));
    fail(&root, &["run", "ms_first"]);
    assert!(root.join("backend/tests/first.rs").is_file());

    write(
        &root,
        "backend/src/lib.rs",
        "pub fn hello() -> &'static str {\n    \"hello\"\n}\n",
    );
    let stdout = ok(&root, &["run", "ms_first"]);
    assert!(stdout.contains("The network says its first word."));
    assert!(
        root.join("solutions/basics/02_journey/ms_first/src/legacy.rs")
            .is_file()
    );

    let (stdout, _) = fail(&root, &["run", "ms_second"]);
    assert!(stdout.contains("Users live in src/users.rs"));

    let stdout = ok(&root, &["journey", "status"]);
    assert!(stdout.contains("1/2"));

    let stdout = ok(&root, &["journey", "baseline", "ms_second"]);
    assert!(stdout.contains("ms_first"));
    assert!(root.join("backend/src/legacy.rs").is_file());
    let backups = fs::read_dir(&root)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(".rustlings-backup-")
        })
        .count();
    assert_eq!(backups, 1);

    write(
        &root,
        "backend/src/users.rs",
        "pub fn count() -> usize {\n    2\n}\n",
    );
    write(
        &root,
        "backend/src/lib.rs",
        "pub mod users;\n\npub fn hello() -> &'static str {\n    \"hello\"\n}\n",
    );
    ok(&root, &["run", "ms_second"]);
    assert!(
        !root
            .join("solutions/basics/02_journey/ms_second/src/legacy.rs")
            .exists()
    );
    assert!(
        root.join("solutions/basics/02_journey/ms_second/src/users.rs")
            .is_file()
    );
    let stdout = ok(&root, &["journey", "status"]);
    assert!(stdout.contains("2/2"));
}

#[test]
fn migration_of_an_old_folder() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    write(
        root,
        "Cargo.toml",
        "bin = [\n  { name = \"test_success\", path = \"exercises/00_intro/test_success.rs\" },\n]\n\n[package]\nname = \"exercises\"\nedition = \"2024\"\n",
    );
    write(
        root,
        ".rustlings-state.txt",
        "DON'T EDIT THIS FILE!\n\ntest_failure\n\ntest_success\ncompilation_success\n",
    );
    let learner = read(&fixture(), "exercises/basics/00_intro/test_success.rs").replace(
        "fn pass() {}",
        "fn pass() {\n        let learner = true;\n    }",
    );
    write(root, "exercises/00_intro/test_success.rs", &learner);
    write(
        root,
        "exercises/00_intro/compilation_success.rs",
        "fn main() {}\n",
    );
    write(root, "exercises/00_intro/README.md", "# old\n");
    write(root, "exercises/quizzes/README.md", "# quizzes\n");
    write(root, "solutions/00_intro/test_success.rs", "placeholder\n");

    let (stdout, _) = fail(root, &["list"]);
    assert!(stdout.contains("rustlings migrate"));

    let stdout = ok(root, &["migrate", "--lang", "ru"]);
    assert!(stdout.contains("test_failure"));

    let migrated = read(root, "exercises/basics/00_intro/test_success.rs");
    assert!(migrated.starts_with("// Здесь уже всё работает.\n"));
    assert!(migrated.contains("        let learner = true;\n"));
    assert!(!root.join("exercises/00_intro").exists());
    assert!(!root.join("exercises/quizzes").exists());
    assert!(
        root.join("exercises/basics/01_proj/proj_sum/Cargo.toml")
            .is_file()
    );
    assert_eq!(
        read(root, "exercises/basics/00_intro/README.md"),
        "# Введение\n\nТеория по-русски.\n"
    );
    assert!(read(root, "solutions/basics/00_intro/test_success.rs").starts_with("// Здесь"));
    assert!(read(root, "Cargo.toml").contains("[workspace]"));
    let state = read(root, ".rustlings-state.txt");
    assert!(state.starts_with("DON'T EDIT THIS FILE!\n\ntest_failure\n"));
    assert!(state.contains("\ncompilation_success") && state.contains("\ntest_success"));

    let originals = read(root, ".rustlings/originals");
    assert!(originals.contains(" exercises/basics/00_intro/test_success.rs\n"));
    assert!(originals.contains(" exercises/basics/00_intro/test_failure.rs\n"));

    let stdout = ok(root, &["run", "test_success"]);
    assert!(stdout.contains("Output from `main` function"));
}

#[test]
fn questions() {
    let (_tmp, root) = init("en");

    let (stdout, _) = fail(&root, &["run", "q_order"]);
    assert!(stdout.contains("What does this print?"));
    assert!(stdout.contains("  2) 2"));
    assert!(stdout.contains("rustlings answer q_order"));

    let (stdout, _) = fail(&root, &["answer", "q_order", "1"]);
    assert!(stdout.contains("Not quite"));
    assert!(stdout.contains("Integers add up"));
    assert!(read(&root, ".rustlings-state.txt").contains("@answer q_order 1"));
    let stdout = ok(&root, &["answer", "q_order", "2"]);
    assert!(stdout.contains("Right!"));
    let state = read(&root, ".rustlings-state.txt");
    assert!(state.contains("\nq_order") && state.contains("@answer q_order 2"));
    assert!(state.contains("@solved q_order "));
    ok(&root, &["run", "q_order"]);

    fail(&root, &["answer", "q_multi", "1"]);
    ok(&root, &["answer", "q_multi", "3,1"]);
    ok(&root, &["answer", "q_text", "  Forty", "TWO "]);
    let (_, stderr) = fail(&root, &["answer", "test_success", "1"]);
    assert!(stderr.contains("not a question"));

    ok(&root, &["lang", "ru"]);
    let (stdout, _) = fail(&root, &["answer", "q_order", "3"]);
    assert!(stdout.contains("Целые числа складываются"));
}

#[test]
fn hint_steps_and_exams() {
    let (_tmp, root) = init("en");
    let stdout = ok(&root, &["hint", "reads_input"]);
    assert!(stdout.contains("Hint 1 of 2") && stdout.contains("First step."));
    assert!(!stdout.contains("Second step."));
    let stdout = ok(&root, &["hint", "reads_input"]);
    assert!(stdout.contains("Hint 2 of 2") && stdout.contains("Second step."));
    assert!(stdout.contains("First step."));
    ok(&root, &["hint", "reads_input"]);
    assert!(read(&root, ".rustlings-state.txt").contains("@hints reads_input 2"));

    let stdout = ok(&root, &["hint", "proj_exam"]);
    assert!(stdout.contains("This is an exam"));
    assert!(!stdout.contains("must never be shown"));

    let stdout = ok(&root, &["hint", "test_failure"]);
    assert!(stdout.contains("The answer to everything: 42"));
    assert!(!stdout.contains("Hint 1 of 1"));
}

#[test]
fn diff_with_reference() {
    let (_tmp, root) = init("en");
    let (stdout, _) = fail(&root, &["diff", "test_failure"]);
    assert!(stdout.contains("after you solve"));
    let path = "exercises/basics/00_intro/test_failure.rs";
    let solved =
        read(&root, path).replace("    // TODO: return the answer.\n    0\n", "    6 * 7\n");
    write(&root, path, &solved);
    ok(&root, &["run", "test_failure"]);
    let stdout = ok(&root, &["diff", "test_failure"]);
    assert!(stdout.contains("- \u{1b}") || stdout.contains("-     6 * 7"));
    assert!(stdout.contains("6 * 7") && stdout.contains("42"));
    assert!(stdout.contains("Returning the value directly"));

    write(
        &root,
        "exercises/basics/01_proj/proj_sum/src/lib.rs",
        "pub fn sum(a: i32, b: i32) -> i32 {\n    b + a\n}\n",
    );
    write(
        &root,
        "exercises/basics/01_proj/proj_sum/NOTES.txt",
        "mine\n",
    );
    ok(&root, &["run", "proj_sum"]);
    let stdout = ok(&root, &["diff", "proj_sum"]);
    assert!(stdout.contains("src/lib.rs (differs)"));
    assert!(stdout.contains("NOTES.txt (only in your code)"));
}

#[test]
fn review_cycle() {
    let (_tmp, root) = init("en");
    let stdout = ok(&root, &["review"]);
    assert!(stdout.contains("Nothing to review yet"));

    let state = "DON'T EDIT THIS FILE!\n\ncompilation_failure\n\ncompilation_success\ntest_success\nreads_input\n\n@solved compilation_success 2020-01-01\n@solved test_success 2020-01-02\n@solved reads_input 2020-01-03\n";
    write(&root, ".rustlings-state.txt", state);

    let stdout = ok(&root, &["review"]);
    assert!(stdout.contains("compilation_success: review/"));
    let state = read(&root, ".rustlings-state.txt");
    let date = state
        .lines()
        .find_map(|l| l.strip_prefix("@review "))
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .to_owned();
    assert!(
        root.join(format!("review/{date}/test_success.rs"))
            .is_file()
    );
    let manifest = read(&root, "tracks/review/Cargo.toml");
    assert!(manifest.contains("name = \"review-reads_input\""));

    write(&root, &format!("review/{date}/test_success.rs"), "broken");
    let (stdout, _) = fail(&root, &["review"]);
    assert!(stdout.contains("2 of 3 solved"));
    let original = read(&fixture(), "exercises/basics/00_intro/test_success.rs");
    write(&root, &format!("review/{date}/test_success.rs"), &original);
    let stdout = ok(&root, &["review"]);
    assert!(stdout.contains("Review complete"));
    assert!(!root.join("tracks/review").exists());
    let state = read(&root, ".rustlings-state.txt");
    assert!(state.contains("@reviewed ") && !state.contains("@review 20"));
    assert!(state.contains("@solved test_success 2020-01-02"));
    ok(&root, &["run", "compilation_success"]);
}

#[test]
fn doctor_reports() {
    let tmp = tempfile::TempDir::new().unwrap();
    let out = rustlings(tmp.path(), &["doctor"]);
    let stdout = from_utf8(&out.stdout).unwrap();
    assert!(stdout.contains("rustc 1."));
    assert!(stdout.contains("postgres"));
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn update_to_a_newer_course_version() {
    let (_tmp, root) = init("en");
    ok(&root, &["run", "compilation_success"]);
    let edited = format!(
        "// my notes\n{}",
        read(&root, "exercises/basics/00_intro/test_failure.rs")
    );
    write(&root, "exercises/basics/00_intro/test_failure.rs", &edited);
    write(
        &root,
        "exercises/basics/01_proj/proj_sum/src/lib.rs",
        "// my project\n",
    );

    let newer = tempfile::TempDir::new().unwrap();
    let content = newer.path();
    copy_dir(&fixture(), content);
    let course_toml = read(content, "course/course.toml").replace(
        "[\"ms_first\", \"ms_second\"]",
        "[\"ms_first\", \"ms_second\", \"ms_third\"]",
    );
    write(content, "course/course.toml", &course_toml);
    let intro = read(content, "exercises/basics/00_intro/chapter.toml")
        + "\n[[exercises]]\nname = \"brand_new\"\ntest = false\n";
    write(content, "exercises/basics/00_intro/chapter.toml", &intro);
    write(
        content,
        "exercises/basics/00_intro/brand_new.rs",
        "fn main() {}\n",
    );
    write(
        content,
        "exercises/basics/00_intro/test_failure.rs",
        "// newer upstream text\nfn main() {}\n",
    );
    write(
        content,
        "exercises/basics/03_not_written/chapter.toml",
        "[chapter]\ntitle = \"Later\"\n\n[[exercises]]\nname = \"later1\"\n\n[[exercises]]\nname = \"proj_later\"\nkind = \"project\"\n",
    );
    write(
        content,
        "exercises/basics/03_not_written/README.md",
        "# Later\n",
    );
    write(
        content,
        "exercises/basics/03_not_written/later1.rs",
        "fn main() {}\n",
    );
    write(
        content,
        "exercises/basics/03_not_written/proj_later/Cargo.toml",
        "[workspace]\n",
    );
    write(
        content,
        "exercises/basics/01_proj/proj_sum/src/extra.rs",
        "pub fn extra() {}\n",
    );
    let journey = read(content, "exercises/basics/02_journey/chapter.toml")
        + "\n[[exercises]]\nname = \"ms_third\"\nkind = \"milestone\"\njourney = \"backend\"\nquiz = true\n";
    write(
        content,
        "exercises/basics/02_journey/chapter.toml",
        &journey,
    );
    write(
        content,
        "exercises/basics/02_journey/ms_third/README.md",
        "# Third\n",
    );
    write(content, "course/support/src/lib.rs", "pub fn newer() {}\n");
    write(
        content,
        "exercises/basics/00_intro/compilation_failure.rs",
        "fn main() {\n    let x\n}\n",
    );
    write(
        content,
        "exercises/basics/00_intro/README.md",
        "# Intro v2\n",
    );

    let out = Command::new(env!("CARGO_BIN_EXE_rustlings"))
        .current_dir(&root)
        .args(["journey", "status"])
        .env("RUSTLINGS_CONTENT_DIR", content)
        .env_remove("CARGO_TARGET_DIR")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", from_utf8(&out.stderr).unwrap());
    let stdout = from_utf8(&out.stdout).unwrap();
    assert!(stdout.contains("0/3") && stdout.contains("ms_third"));

    assert_eq!(
        read(&root, "exercises/basics/00_intro/brand_new.rs"),
        "fn main() {}\n"
    );
    assert_eq!(
        read(&root, "exercises/basics/03_not_written/README.md"),
        "# Later\n"
    );
    assert!(
        root.join("exercises/basics/03_not_written/later1.rs")
            .is_file()
    );
    assert!(
        root.join("exercises/basics/03_not_written/proj_later/Cargo.toml")
            .is_file()
    );
    assert!(
        root.join("exercises/basics/02_journey/ms_third/README.md")
            .is_file()
    );
    let track = read(&root, "tracks/basics/Cargo.toml");
    assert!(track.contains("name = \"brand_new\"") && track.contains("name = \"later1\""));
    assert_eq!(read(&root, "support/src/lib.rs"), "pub fn newer() {}\n");
    assert!(read(&root, ".vscode/settings.json").contains("proj_later/Cargo.toml"));

    assert_eq!(
        read(&root, "exercises/basics/00_intro/test_failure.rs"),
        edited
    );
    assert_eq!(
        read(&root, "exercises/basics/01_proj/proj_sum/src/lib.rs"),
        "// my project\n"
    );
    assert!(
        root.join("exercises/basics/01_proj/proj_sum/src/extra.rs")
            .is_file()
    );
    assert_eq!(
        read(&root, "exercises/basics/00_intro/compilation_failure.rs"),
        "fn main() {\n    let x\n}\n"
    );
    assert_eq!(
        read(&root, "exercises/basics/00_intro/README.md"),
        "# Intro v2\n"
    );
    let originals = read(&root, ".rustlings/originals");
    assert!(originals.contains(" exercises/basics/00_intro/brand_new.rs\n"));
    let state = read(&root, ".rustlings-state.txt");
    assert!(state.contains("\ncompilation_success"));
}
