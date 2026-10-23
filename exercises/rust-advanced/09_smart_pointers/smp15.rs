// Справочник кодов и счётчик обращений живут в глобальных переменных, чтобы не таскать их через все
// функции. Справочник не собирается: «cannot call non-const method `HashMap::insert` in statics», в
// статик можно положить только то, что вычисляется при сборке. Счётчик сделан через `static mut` с `unsafe`,
// а это обещание «я вручную слежу, чтобы к нему не обращались из двух потоков сразу», и три потока
// из теста его нарушают. Нужен безопасный способ: значение, которое создаётся при первом использовании,
// и число, которое можно менять из любого потока.
// TODO: Замени оба статика безопасными (без `unsafe` и без `static mut`).
use std::collections::HashMap;
static CODES: HashMap<&str, u32> = {
    let mut m = HashMap::new();
    m.insert("ok", 200);
    m.insert("missing", 404);
    m.insert("teapot", 418);
    m
};

static mut CALLS: u32 = 0;

pub fn code_of(name: &str) -> Option<u32> {
    unsafe {
        CALLS += 1;
    }
    CODES.get(name).copied()
}

pub fn calls() -> u32 {
    unsafe { CALLS }
}

fn main() {
    println!("{:?} {}", code_of("ok"), calls());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::thread;

    const SRC: &str = include_str!("smp15.rs");

    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn lookups_work() {
        let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(code_of("ok"), Some(200));
        assert_eq!(code_of("teapot"), Some(418));
        assert_eq!(code_of("nope"), None);
    }

    #[test]
    fn counting_survives_many_threads() {
        let _turn = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let before = calls();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                thread::spawn(|| {
                    for _ in 0..2000 {
                        code_of("ok");
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().expect("поток");
        }
        assert_eq!(calls() - before, 16_000, "ни один вызов не должен потеряться: счётчик общий для потоков");
    }

    #[test]
    fn no_unsafe_left() {
        let code = SRC.split("#[cfg(test)]").next().unwrap_or("");
        let code: String = code.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        assert!(!code.contains("static mut"), "static mut убери: нужна безопасная замена");
        assert!(!code.contains("unsafe"), "в решении не должно остаться unsafe");
    }
}
