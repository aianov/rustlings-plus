pub fn take_damage(hp: u8, damage: u8) -> u8 {
    hp.saturating_sub(damage)
}

pub fn next_frame(frame: u8) -> u8 {
    frame.wrapping_add(1)
}

pub fn total_size(sizes: &[u32]) -> Option<u32> {
    sizes.iter().try_fold(0u32, |total, &size| total.checked_add(size))
}

pub fn midpoint(a: u8, b: u8) -> u8 {
    ((u16::from(a) + u16::from(b)) / 2) as u8
}

pub fn magnitude(x: i8) -> Option<i8> {
    x.checked_abs()
}

pub fn safe_div(a: i32, b: i32) -> Option<i32> {
    a.checked_div(b)
}

pub fn add_with_flag(a: u8, b: u8) -> (u8, bool) {
    a.overflowing_add(b)
}

pub fn fnv1a(bytes: &[u8]) -> u32 {
    let mut hash = 2_166_136_261u32;
    for &byte in bytes {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}

fn main() {
    println!("{}", take_damage(10, 20));
    println!("{}", next_frame(255));
    println!("{:?}", total_size(&[u32::MAX, 1]));
    println!("{}", midpoint(200, 100));
    println!("{:?}", magnitude(i8::MIN));
    println!("{:?}", safe_div(i32::MIN, -1));
    println!("{:?}", add_with_flag(250, 10));
    println!("{:08x}", fnv1a(b"hello"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_stops_at_zero() {
        assert_eq!(take_damage(100, 30), 70, "обычный урон");
        assert_eq!(take_damage(10, 20), 0, "здоровье не уходит ниже нуля: насыщение, а не заворот на 246");
    }

    #[test]
    fn frame_counter_wraps_on_purpose() {
        assert_eq!(next_frame(41), 42, "обычный кадр");
        assert_eq!(next_frame(255), 0, "после 255 счётчик кадров начинается с нуля, и это задумано");
    }

    #[test]
    fn total_size_reports_overflow() {
        assert_eq!(total_size(&[1, 2, 3]), Some(6), "обычная сумма");
        assert_eq!(total_size(&[]), Some(0), "пустая сумма это ноль");
        assert_eq!(total_size(&[u32::MAX, 1]), None, "переполнение суммы нельзя прятать: нужен None");
    }

    #[test]
    fn midpoint_does_not_overflow() {
        assert_eq!(midpoint(10, 20), 15, "обычное среднее");
        assert_eq!(midpoint(200, 100), 150, "200 + 100 не помещается в u8, а среднее помещается");
        assert_eq!(midpoint(255, 255), 255, "среднее двух одинаковых это они сами");
    }

    #[test]
    fn magnitude_of_min_is_none() {
        assert_eq!(magnitude(-5), Some(5), "модуль -5");
        assert_eq!(magnitude(i8::MIN), None, "модуль -128 не помещается в i8");
    }

    #[test]
    fn division_edge_cases() {
        assert_eq!(safe_div(7, 2), Some(3), "обычное деление");
        assert_eq!(safe_div(1, 0), None, "деление на ноль");
        assert_eq!(safe_div(i32::MIN, -1), None, "i32::MIN / -1 переполняется, это тоже None");
    }

    #[test]
    fn overflow_flag_is_reported() {
        assert_eq!(add_with_flag(1, 2), (3, false), "без переполнения флаг пуст");
        assert_eq!(add_with_flag(250, 10), (4, true), "250 + 10 даёт 4 по модулю 256 и флаг переполнения");
    }

    #[test]
    fn fnv1a_matches_reference_values() {
        assert_eq!(fnv1a(b""), 0x811c_9dc5, "хеш пустой строки равен начальному значению");
        assert_eq!(fnv1a(b"a"), 0xe40c_292c, "эталонный FNV-1a от \"a\"");
        assert_eq!(fnv1a(b"foobar"), 0xbf9c_f968, "эталонный FNV-1a от \"foobar\": умножение в хеше должно заворачиваться");
    }
}
