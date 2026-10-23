use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

struct Run {
    success: bool,
    output: String,
    timed_out: bool,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn student_suite(features: &str) -> Run {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut command = Command::new(cargo);
    command.current_dir(root()).args(["test", "-p", "notifier", "--test", "student", "--color", "never"]);
    if !features.is_empty() {
        command.args(["--features", features]);
    }
    let out = std::env::temp_dir().join(format!("tt-mutants-{}-{}.log", std::process::id(), features.replace(',', "-")));
    let file = std::fs::File::create(&out).unwrap();
    let mut child = command.stdout(Stdio::from(file.try_clone().unwrap())).stderr(Stdio::from(file)).spawn().expect("не удалось запустить cargo");
    let started = Instant::now();
    let (success, timed_out) = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break (status.success(), false);
        }
        if started.elapsed() > Duration::from_secs(300) {
            let _ = child.kill();
            let _ = child.wait();
            break (false, true);
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let output = std::fs::read_to_string(&out).unwrap_or_default();
    let _ = std::fs::remove_file(&out);
    Run { success, output, timed_out }
}

fn tests_run(output: &str) -> usize {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("test result:"))
        .filter_map(|rest| rest.split_whitespace().nth(1).and_then(|n| n.parse::<usize>().ok()))
        .sum()
}

#[test]
fn your_tests_pass_on_the_correct_service() {
    let run = student_suite("");
    assert!(!run.timed_out, "твои тесты не закончились за 5 минут: ждут настоящего времени или зависли");
    assert!(run.success, "на исправном сервисе твои тесты обязаны проходить. Вывод cargo:\n{}", run.output);
    let count = tests_run(&run.output);
    assert!(count >= 6, "нужно хотя бы шесть тестов, а найдено {count}: сервис ведёт себя по-разному в разных случаях");
}

fn must_be_caught(feature: &str, symptom: &str) {
    let run = student_suite(feature);
    assert!(!run.timed_out, "на сломанной версии `{feature}` твои тесты зависли: поставь таймауты или замороженное время");
    assert!(!run.output.contains("could not compile"), "версия `{feature}` не собралась вместе с твоими тестами:\n{}", run.output);
    assert!(
        !run.success && run.output.contains("test result: FAILED"),
        "сломанная версия `{feature}` прошла твои тесты. Её симптом: {symptom}. Напиши тест, который это ловит."
    );
}

#[test]
fn bug_overwrite_is_caught() {
    must_be_caught("bug_overwrite", "вторая запись в окне затирает первую");
}

#[test]
fn bug_debounce_is_caught() {
    must_be_caught("bug_debounce", "каждая новая запись отодвигает закрытие окна");
}

#[test]
fn bug_off_by_one_is_caught() {
    must_be_caught("bug_off_by_one", "дайджест вмещает на одну запись больше лимита");
}

#[test]
fn bug_lost_tail_is_caught() {
    must_be_caught("bug_lost_tail", "при остановке накопленное пропадает");
}

#[test]
fn bug_mute_pending_is_caught() {
    must_be_caught("bug_mute_pending", "после mute уходит уже накопленный дайджест");
}

#[test]
fn bug_order_is_caught() {
    must_be_caught("bug_order", "записи в дайджесте идут не в порядке прихода");
}

#[test]
fn bug_leak_is_caught() {
    must_be_caught("bug_leak", "состояние пользователя остаётся после доставки");
}

#[test]
fn bug_closed_ok_is_caught() {
    must_be_caught("bug_closed_ok", "после остановки notify сообщает об успехе");
}
