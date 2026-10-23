// Скрипту разработчика часто нужно запустить другую программу: cargo, git, docker. Для этого есть
// std::process::Command. У запуска два разных вида неудачи: программу не удалось запустить вообще
// (её нет в PATH) и программа запустилась, но завершилась с ненулевым кодом. Это разные ошибки,
// и скрипт должен их различать.
// TODO: Реализуй `run`: выполни программу, дождись, верни её stdout без пробелов по краям. Ненулевой
// код и пропавшую программу верни как разные варианты `RunError`. Объяснение ошибки (stderr) тоже
// приходит без пробелов по краям, а в `Spawn` нужно имя программы, которую не удалось запустить.

use std::path::Path;

#[derive(Debug)]
pub enum RunError {
    Spawn(String),
    Failed { code: Option<i32>, stderr: String },
}

pub fn run(program: &str, args: &[&str], dir: Option<&Path>) -> Result<String, RunError> {
    todo!("запустить программу в папке dir (если задана) и собрать вывод")
}

fn main() {
    match run("cargo", &["--version"], None) {
        Ok(version) => println!("{version}"),
        Err(error) => eprintln!("{error:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_command_returns_trimmed_stdout() {
        let out = run("cargo", &["--version"], None).expect("cargo --version должен работать");
        assert!(out.starts_with("cargo "), "ожидали строку версии, получили {out:?}");
        assert!(!out.ends_with('\n'), "перевод строки в конце нужно обрезать");
    }

    #[test]
    fn only_stdout_is_returned() {
        let out = run("sh", &["-c", "echo out; echo err >&2"], None).expect("sh должен работать");
        assert_eq!(out, "out", "stderr в результат попадать не должен");
    }

    #[test]
    fn failing_command_is_an_error_with_code_and_stderr() {
        match run("sh", &["-c", "echo boom >&2; exit 3"], None) {
            Err(RunError::Failed { code, stderr }) => {
                assert_eq!(code, Some(3), "код возврата должен сохраниться");
                assert_eq!(stderr, "boom", "stderr упавшей команды нужен для сообщения об ошибке");
            }
            other => panic!("ненулевой код должен давать Failed, а получилось {other:?}"),
        }
    }

    #[test]
    fn real_cargo_failure_is_a_failure_too() {
        match run("cargo", &["definitely-not-a-subcommand"], None) {
            Err(RunError::Failed { code, stderr }) => {
                assert_ne!(code, Some(0));
                assert!(!stderr.is_empty(), "cargo объясняет ошибку в stderr");
            }
            other => panic!("неизвестная подкоманда cargo должна давать Failed, а получилось {other:?}"),
        }
    }

    #[test]
    fn missing_program_is_a_different_error() {
        match run("no-such-program-for-the-course", &[], None) {
            Err(RunError::Spawn(message)) => {
                assert!(message.contains("no-such-program-for-the-course"), "в сообщении должно быть имя программы: {message}");
            }
            other => panic!("пропавшая программа должна давать Spawn, а получилось {other:?}"),
        }
    }

    #[test]
    fn command_runs_in_the_requested_folder() {
        let dir = std::env::temp_dir().canonicalize().expect("временная папка существует");
        let out = run("sh", &["-c", "pwd"], Some(&dir)).expect("pwd должен работать");
        let reported = std::path::PathBuf::from(out).canonicalize().expect("путь существует");
        assert_eq!(reported, dir, "команда должна выполняться в переданной папке");
    }
}
