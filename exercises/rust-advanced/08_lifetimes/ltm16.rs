// Конвейер чистит строки пользовательскими функциями вида «взять `&str`, вернуть кусок этого же `&str`»:
// `str::trim`, «первое слово», «до запятой». `apply_to_all` перед вызовом переводит каждую строку в верхний
// регистр, то есть передаёт функции ссылку на локальную строку, которая живёт только внутри одной итерации.
// Не собирается: «`upper` does not live long enough». Условие `F: Fn(&'a str) -> &'a str` с `'a`,
// объявленным на самой функции, требует одно фиксированное время жизни на все вызовы, а нам нужно «любое».
// Вторая функция, `first_where`, принимает предикат. Для неё условие напиши с нуля.
// TODO: Почини условие в `apply_to_all` и допиши `first_where` (вернуть первый подходящий элемент).

pub fn apply_to_all<'a, F>(inputs: &[&str], f: F) -> Vec<String>
where
    F: Fn(&'a str) -> &'a str,
{
    inputs
        .iter()
        .map(|s| {
            let upper = s.to_uppercase();
            f(&upper).to_string()
        })
        .collect()
}

pub fn first_where<P>(items: &[String], pred: P) -> Option<&str> {
    todo!("первый элемент, для которого pred вернул true")
}

fn main() {
    println!("{:?}", apply_to_all(&[" a "], str::trim));
}

#[cfg(test)]
mod tests {
    use super::*;

        const SRC: &str = include_str!("ltm16.rs");

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


    #[test]
    fn cleans_with_a_function_pointer() {
        assert_eq!(apply_to_all(&["  hello ", "world  "], str::trim), vec!["HELLO", "WORLD"]);
    }

    #[test]
    fn cleans_with_a_closure() {
        assert_eq!(apply_to_all(&["a,b", "c"], |s| s.split(',').next().unwrap_or(s)), vec!["A", "C"]);
        assert_eq!(apply_to_all(&[], str::trim), Vec::<String>::new());
    }

    #[test]
    fn closures_that_ignore_the_input_are_fine() {
        assert_eq!(apply_to_all(&["x", "y"], |_| "const"), vec!["const", "const"], "результат 'static подходит под любое время жизни");
    }

    #[test]
    fn finds_the_first_match() {
        let items = vec!["apple".to_string(), "banana".to_string(), "blueberry".to_string()];
        assert_eq!(first_where(&items, |s| s.starts_with('b')), Some("banana"));
        assert_eq!(first_where(&items, |s| s.len() > 20), None);
        assert_eq!(first_where(&[], |_| true), None);
    }

    #[test]
    fn the_predicate_gets_any_borrowed_string_and_the_result_outlives_it() {
        assert!(
            compiles_with("pub fn t(items: &[String]) -> Option<&str> { let needle = String::from(\"b\"); first_where(items, |s| s.starts_with(needle.as_str())) }"),
            "результат first_where берётся из items, а не из временных значений предиката"
        );
    }

    #[test]
    fn filters_are_callable_with_strings_of_different_scopes() {
        assert!(
            compiles_with("pub fn t() -> usize { let a = String::from(\"  x \"); let r1 = apply_to_all(&[a.as_str()], str::trim); { let b = String::from(\" y\"); let r2 = apply_to_all(&[b.as_str()], str::trim); r1.len() + r2.len() } }"),
            "функция-обработчик должна подходить для ссылок с любым временем жизни"
        );
    }
}
