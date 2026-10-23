use std::fmt::Display;

pub struct Labeled<T> {
    pub label: &'static str,
    value: T,
}

impl<T> Labeled<T> {
    pub fn new(label: &'static str, value: T) -> Self {
        Labeled { label, value }
    }

    pub fn into_value(self) -> T {
        self.value
    }
}

impl<T: Display> Labeled<T> {
    pub fn render(&self) -> String {
        format!("{}: {}", self.label, self.value)
    }
}

impl<T: Display + PartialOrd> Labeled<T> {
    pub fn render_over(&self, limit: &T) -> Option<String> {
        (self.value > *limit).then(|| self.render())
    }
}

impl Labeled<i32> {
    pub fn is_negative(&self) -> bool {
        self.value < 0
    }
}

fn main() {
    let item = Labeled::new("temp", -4);
    println!("{} {}", item.render(), item.is_negative());
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = include_str!("gnc2.rs");

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
    fn render_for_display_types() {
        assert_eq!(Labeled::new("n", 42).render(), "n: 42");
        assert_eq!(Labeled::new("name", "ann").render(), "name: ann");
        assert_eq!(Labeled::new("ratio", 0.5).render(), "ratio: 0.5");
    }

    #[test]
    fn storing_works_without_display() {
        let blob = Labeled::new("raw", vec![1u8, 2, 3]);
        assert_eq!(blob.into_value(), vec![1, 2, 3], "хранить и возвращать данные можно у любого типа");
    }

    #[test]
    fn render_is_absent_without_display() {
        assert!(
            !compiles_with("fn p(l: Labeled<Vec<u8>>) -> String { l.render() }"),
            "у Labeled<Vec<u8>> не должно быть render: у Vec<u8> нет Display"
        );
        assert!(compiles_with("fn p(l: Labeled<u8>) -> String { l.render() }"), "а у Labeled<u8> должен быть");
    }

    #[test]
    fn is_negative_only_for_i32() {
        assert!(Labeled::new("t", -3).is_negative());
        assert!(!Labeled::new("t", 0).is_negative());
        assert!(
            !compiles_with("fn p(l: Labeled<f64>) -> bool { l.is_negative() }"),
            "is_negative написан только для Labeled<i32>, у Labeled<f64> его нет"
        );
    }

    #[test]
    fn render_over_needs_order_and_display() {
        assert_eq!(Labeled::new("n", 10).render_over(&5), Some("n: 10".to_string()));
        assert_eq!(Labeled::new("n", 5).render_over(&5), None, "строго больше порога");
        assert_eq!(Labeled::new("w", "pear").render_over(&"apple"), Some("w: pear".to_string()));
        assert!(
            !compiles_with("struct NoOrd; impl std::fmt::Display for NoOrd { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str(\"x\") } } fn p(l: Labeled<NoOrd>) { l.render_over(&NoOrd); }"),
            "render_over требует PartialOrd: без него вызов не должен собираться"
        );
    }
}
