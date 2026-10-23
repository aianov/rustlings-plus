mod fastparse {
    #[derive(Debug)]
    pub(crate) struct Failure {
        pub(crate) offset: usize,
    }

    pub(crate) fn parse_pairs(text: &str) -> Result<Vec<(String, String)>, Failure> {
        let mut pairs = Vec::new();
        let mut offset = 0;
        for line in text.lines() {
            let (key, value) = line.split_once('=').ok_or(Failure { offset })?;
            pairs.push((key.trim().to_string(), value.trim().to_string()));
            offset += line.len() + 1;
        }
        Ok(pairs)
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Syntax { offset: usize },
    Empty,
}

pub fn load(text: &str) -> Result<Vec<(String, String)>, ConfigError> {
    let pairs = fastparse::parse_pairs(text).map_err(|f| ConfigError::Syntax { offset: f.offset })?;
    if pairs.is_empty() {
        return Err(ConfigError::Empty);
    }
    Ok(pairs)
}

fn main() {
    println!("{:?}", load("a=1").map(|p| p.len()));
}

#[cfg(test)]
mod tests {
    use super::*;

        const SRC: &str = include_str!("erp11.rs");

    fn compiles_with_lints(extra: &str, deny: &[&str]) -> bool {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("rustlings_probe_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).expect("не удалось создать папку");
        let file = dir.join("probe.rs");
        let code = SRC.split("#[cfg(test)]").next().unwrap_or("");
        std::fs::write(&file, format!("{code}\n{extra}\n")).expect("не удалось записать файл");
        let mut cmd = std::process::Command::new("rustc");
        cmd.args(["--edition", "2024", "--crate-type", "lib", "--emit=metadata", "-A", "warnings"]);
        for lint in deny {
            cmd.args(["-D", lint]);
        }
        let ok = cmd.arg("--out-dir").arg(&dir).arg(&file).output().expect("не удалось запустить rustc").status.success();
        let _ = std::fs::remove_dir_all(&dir);
        ok
    }


    #[test]
    fn loads_good_text() {
        let pairs = load("host = local\nport=80").unwrap();
        assert_eq!(pairs, vec![("host".to_string(), "local".to_string()), ("port".to_string(), "80".to_string())]);
    }

    #[test]
    fn syntax_error_reports_the_offset() {
        let err = load("a=1\nbad\nc=3").unwrap_err();
        assert!(matches!(err, ConfigError::Syntax { offset: 4 }), "ошибка на второй строке, байт 4: {err:?}");
    }

    #[test]
    fn empty_input_is_its_own_error() {
        assert!(matches!(load(""), Err(ConfigError::Empty)));
    }

    #[test]
    fn public_api_names_no_private_types() {
        assert!(
            compiles_with_lints("pub fn probe() {}", &["private_interfaces", "private_bounds"]),
            "публичный интерфейс называет тип, о котором пользователи крейта ничего не знают и знать не должны"
        );
    }
}
