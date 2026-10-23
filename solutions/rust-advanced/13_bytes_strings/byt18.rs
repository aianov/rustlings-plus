use std::fmt;

pub struct Elapsed(pub u64);

pub fn row(name: &str, likes: u32, ratio: f64) -> String {
    format!("{name:<8.8}|{likes:>6}|{ratio:>7.2}")
}

pub fn flags_line(flags: u8) -> String {
    format!("flags={flags:#010b} ({flags:#04x})")
}

pub fn id_label(id: u64) -> String {
    format!("{id:#06x}")
}

impl fmt::Display for Elapsed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (h, m, s) = (self.0 / 3600, self.0 / 60 % 60, self.0 % 60);
        let text = if h > 0 {
            format!("{h}h {m:02}m {s:02}s")
        } else if m > 0 {
            format!("{m}m {s:02}s")
        } else {
            format!("{s}s")
        };
        f.pad(&text)
    }
}

fn main() {
    println!("{}", row("ira", 42, 1.4567));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_aligned() {
        assert_eq!(row("ira", 42, 1.4567), "ira     |    42|   1.46");
        assert_eq!(row("alexandra-long", 1_000_000, 99.999), "alexandr|1000000| 100.00", "длинное имя обрезается, широкое число не обрезается");
        assert_eq!(row("", 0, 0.0), "        |     0|   0.00");
    }

    #[test]
    fn flags_show_binary_and_hex() {
        assert_eq!(flags_line(5), "flags=0b00000101 (0x05)");
        assert_eq!(flags_line(255), "flags=0b11111111 (0xff)");
        assert_eq!(flags_line(0), "flags=0b00000000 (0x00)");
    }

    #[test]
    fn ids_have_a_hex_label() {
        assert_eq!(id_label(255), "0x00ff");
        assert_eq!(id_label(0), "0x0000");
        assert_eq!(id_label(0xabcdef), "0xabcdef", "длинное число не обрезается");
    }

    #[test]
    fn elapsed_picks_units() {
        assert_eq!(Elapsed(5).to_string(), "5s");
        assert_eq!(Elapsed(65).to_string(), "1m 05s");
        assert_eq!(Elapsed(3725).to_string(), "1h 02m 05s");
        assert_eq!(Elapsed(0).to_string(), "0s");
        assert_eq!(Elapsed(7200).to_string(), "2h 00m 00s");
    }

    #[test]
    fn elapsed_respects_width_and_alignment() {
        assert_eq!(format!("[{:>8}]", Elapsed(65)), "[  1m 05s]");
        assert_eq!(format!("[{:<8}]", Elapsed(5)), "[5s      ]");
        assert_eq!(format!("[{:*^9}]", Elapsed(65)), "[*1m 05s**]", "ширина и заполнитель из формата должны работать и для своего типа");
    }
}
