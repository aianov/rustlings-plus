use std::hint::black_box;
use std::time::{Duration, Instant};

pub fn checksum(data: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub fn time_checksum(data: &[u8], rounds: u32) -> Duration {
    let start = Instant::now();
    for _ in 0..rounds {
        black_box(checksum(black_box(data)));
    }
    start.elapsed() / rounds
}

fn main() {
    let data = vec![7u8; 1 << 20];
    println!("{}", time_checksum(&data, 20).as_nanos());
}

#[cfg(test)]
mod tests {
    use super::*;
    const SRC: &str = include_str!("perf9.rs");

    fn release_nanos_per_round() -> u128 {
        let dir = std::env::temp_dir().join(format!("perf9_release_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("не удалось создать папку");
        let file = dir.join("bench.rs");
        let code = SRC.split("#[cfg(test)]\nmod tests").next().unwrap_or("");
        std::fs::write(&file, code).expect("не удалось записать файл");
        let binary = dir.join("bench");
        let built = std::process::Command::new("rustc")
            .args(["--edition", "2024", "-O", "-A", "warnings", "-o"])
            .arg(&binary)
            .arg(&file)
            .output()
            .expect("не удалось запустить rustc");
        assert!(built.status.success(), "релизная сборка не собралась:\n{}", String::from_utf8_lossy(&built.stderr));
        let run = std::process::Command::new(&binary).output().expect("не удалось запустить программу");
        let _ = std::fs::remove_dir_all(&dir);
        String::from_utf8_lossy(&run.stdout).trim().parse().expect("программа должна напечатать одно число: наносекунд на круг")
    }

    #[test]
    fn checksum_is_fnv() {
        assert_eq!(checksum(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(checksum(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(checksum(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn timing_returns_a_duration() {
        let data = vec![1u8; 1000];
        let per_round = time_checksum(&data, 4);
        assert!(per_round < Duration::from_secs(5));
    }

    #[test]
    fn release_build_really_does_the_work() {
        let nanos = release_nanos_per_round();
        assert!(
            nanos > 50_000,
            "в релизной сборке круг по мегабайту занял {nanos} нс: так быстро хешировать нельзя, оптимизатор выбросил работу"
        );
    }
}
