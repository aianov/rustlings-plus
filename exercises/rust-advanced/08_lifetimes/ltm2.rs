// Командный интерпретатор ищет команду по имени. Реестр живёт всю программу, а имя пришло из строки ввода,
// которая через секунду будет затёрта следующей. Ничто не мешает вернуть ссылку на команду из реестра
// и спокойно её использовать, когда строка уже исчезла, но сигнатура сейчас не собирается: у функции два
// входа-ссылки, и компилятор не знает, из какого из них результат. Та же история у `after_prefix`: берёт
// взаймы у строки, а префикс нужен только на время вызова.
// TODO: Расставь времена жизни так, чтобы результат зависел только от реестра и от строки соответственно.

pub struct Command {
    pub name: String,
    pub help: String,
}

pub fn find_command(registry: &[Command], name: &str) -> Option<&Command> {
    registry.iter().find(|c| c.name == name)
}

pub fn after_prefix(line: &str, prefix: &str) -> Option<&str> {
    line.strip_prefix(prefix).map(str::trim_start)
}

fn main() {
    let registry = vec![Command { name: "help".into(), help: "show help".into() }];
    println!("{:?}", find_command(&registry, "help").map(|c| &c.help));
    println!("{:?}", after_prefix("/say hi", "/say"));
}

#[cfg(test)]
mod tests {
    use super::*;

        const SRC: &str = include_str!("ltm2.rs");

    fn compiles_with(extra: &str) -> bool {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("rustlings_probe_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).expect("не удалось создать папку");
        let file = dir.join("probe.rs");
        let code = SRC.split("#[cfg(test)]").next().unwrap_or("");
        std::fs::write(&file, format!("{code}\n{extra}\n")).expect("не удалось записать файл");
        let ok = std::process::Command::new("rustc")
            .args(["--edition", "2024", "--crate-type", "lib", "--emit=metadata", "-A", "warnings", "--out-dir"])
            .arg(&dir)
            .arg(&file)
            .output()
            .expect("не удалось запустить rustc")
            .status
            .success();
        let _ = std::fs::remove_dir_all(&dir);
        ok
    }


    fn registry() -> Vec<Command> {
        vec![
            Command { name: "help".into(), help: "show help".into() },
            Command { name: "quit".into(), help: "leave".into() },
        ]
    }

    #[test]
    fn finds_commands_by_name() {
        let reg = registry();
        assert_eq!(find_command(&reg, "quit").map(|c| c.help.as_str()), Some("leave"));
        assert!(find_command(&reg, "nope").is_none());
    }

    #[test]
    fn cuts_the_prefix_and_following_spaces() {
        assert_eq!(after_prefix("/say   hello world", "/say"), Some("hello world"));
        assert_eq!(after_prefix("/say", "/say"), Some(""));
        assert_eq!(after_prefix("hello", "/say"), None);
    }

    #[test]
    fn the_name_may_die_before_the_result() {
        assert!(
            compiles_with("pub fn t1(r: &[Command]) -> Option<&Command> { let hit; { let typed = String::from(\"help\"); hit = find_command(r, &typed); } hit }"),
            "результат find_command должен зависеть только от реестра: строка с именем живёт недолго, а команда после неё ещё нужна"
        );
        assert!(
            compiles_with("pub fn t2(line: &str) -> Option<&str> { let rest; { let prefix = String::from(\"/say\"); rest = after_prefix(line, &prefix); } rest }"),
            "результат after_prefix режется из line, префикс нужен только на время вызова"
        );
    }
}
