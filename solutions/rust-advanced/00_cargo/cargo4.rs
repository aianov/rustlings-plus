#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct Config {
    pub name: String,
    pub retries: u32,
}

#[cfg(test)]
fn sample_config() -> Config {
    Config { name: "demo".to_string(), retries: 3 }
}

pub fn build_mode() -> &'static str {
    if cfg!(debug_assertions) { "debug" } else { "release" }
}

#[cfg(debug_assertions)]
pub fn log_line(msg: &str) -> Option<String> {
    Some(format!("[debug] {msg}"))
}

#[cfg(not(debug_assertions))]
pub fn log_line(_msg: &str) -> Option<String> {
    None
}

pub fn retry_delay_ms(attempt: u32) -> u64 {
    debug_assert!(attempt <= 16, "attempt {attempt} выглядит как баг вызывающего кода");
    100u64 << attempt.min(16)
}

fn main() {
    let config = Config { name: "demo".to_string(), retries: 3 };
    println!("{} ({} retries), {}", config.name, config.retries, build_mode());
    if let Some(line) = log_line("started") {
        println!("{line}");
    }
    println!("{}", retry_delay_ms(config.retries));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_matches_the_profile() {
        let expected = if cfg!(debug_assertions) { "debug" } else { "release" };
        assert_eq!(build_mode(), expected, "build_mode должна сказать, в каком профиле собрана программа");
    }

    #[test]
    fn config_can_be_compared_in_tests() {
        let expected = Config { name: "demo".to_string(), retries: 3 };
        assert_eq!(sample_config(), expected, "sample_config должна вернуть Config {{ name: \"demo\", retries: 3 }}");
    }

    #[test]
    fn log_line_exists_only_in_debug() {
        if cfg!(debug_assertions) {
            assert_eq!(log_line("cache miss").as_deref(), Some("[debug] cache miss"), "в debug строка лога имеет вид `[debug] текст`");
        } else {
            assert_eq!(log_line("cache miss"), None, "в release отладочный лог должен молчать");
        }
    }

    #[test]
    fn retry_delay_doubles() {
        assert_eq!(retry_delay_ms(0), 100, "первая попытка ждёт 100 мс");
        assert_eq!(retry_delay_ms(3), 800, "четвёртая ждёт 800 мс");
        assert_eq!(retry_delay_ms(16), 100 << 16, "потолок на шестнадцатой попытке");
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "attempt")]
    fn absurd_attempt_is_a_bug_in_debug() {
        retry_delay_ms(40);
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn absurd_attempt_is_clamped_in_release() {
        assert_eq!(retry_delay_ms(40), 100 << 16, "в release лишние попытки не растят задержку выше потолка");
    }

    const SRC: &str = include_str!("cargo4.rs");

    fn production() -> &'static str {
        SRC.rsplit_once("#[cfg(test)]\nmod tests").map_or(SRC, |(code, _)| code)
    }

    #[test]
    fn conditional_compilation_is_used_where_it_belongs() {
        let code = production();
        assert!(code.contains("debug_assert!("), "невозможное значение в retry_delay_ms ловит debug_assert!, обычный assert! стоил бы денег и в release");
        assert!(code.contains("cfg_attr(test"), "derive только для тестов пишется через #[cfg_attr(test, derive(...))]");
        assert!(
            code.contains("#[cfg(debug_assertions)]") && code.contains("#[cfg(not(debug_assertions))]"),
            "log_line должна быть двумя функциями с #[cfg(debug_assertions)] и #[cfg(not(debug_assertions))], а не веткой if cfg!"
        );
    }

    #[test]
    fn release_flavour_passes_too() {
        if std::env::var_os("CARGO4_NESTED").is_some() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("cargo4_release_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("не удалось создать временную папку");
        let exe = dir.join("release_tests");
        let src = dir.join("cargo4.rs");
        std::fs::write(&src, SRC).expect("не удалось записать файл");
        let build = std::process::Command::new("rustc")
            .args(["--edition", "2024", "--test", "-C", "debug-assertions=off", "-C", "overflow-checks=off", "-o"])
            .arg(&exe)
            .arg(&src)
            .output()
            .expect("не удалось запустить rustc");
        assert!(build.status.success(), "в режиме без debug_assertions код не собирается:\n{}", String::from_utf8_lossy(&build.stderr));
        let run = std::process::Command::new(&exe).env("CARGO4_NESTED", "1").output().expect("не удалось запустить тесты");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            run.status.success(),
            "тесты падают в режиме release (без debug_assertions):\n{}",
            String::from_utf8_lossy(&run.stdout)
        );
    }
}
