// Три функции складывают числа из байт и режут биты, и все три врут. Первая собирает `u16` из двух байт и
// теряет приоритет операторов: сдвиг и сложение в Rust, в отличие от школьной арифметики, не так
// упорядочены, как кажется. Вторая собирает `u32` и ломается, когда последний байт больше 127. Третья
// берёт младшие `n` бит и падает на `n = 32`. Все баги из тех, что пропускает тест на «обычных» значениях.
// TODO: Почини три функции. Тесты проверяют краевые значения.
pub fn be_u16(hi: u8, lo: u8) -> u16 {
    (hi as u16) << 8 + lo as u16
}

pub fn be_u32(b: [u8; 4]) -> u32 {
    ((b[0] as u32) << 24) | ((b[1] as u32) << 16) | ((b[2] as u32) << 8) | (b[3] as i8) as u32
}

pub fn low_bits(value: u32, n: u32) -> u32 {
    value & ((1u32 << n) - 1)
}

fn main() {
    println!("{}", be_u16(1, 2));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_bytes_make_a_number() {
        assert_eq!(be_u16(0, 0), 0);
        assert_eq!(be_u16(0x12, 0x34), 0x1234);
        assert_eq!(be_u16(0xff, 0xff), 0xffff);
        assert_eq!(be_u16(0x01, 0x80), 0x0180);
    }

    #[test]
    fn four_bytes_make_a_number_for_every_value() {
        assert_eq!(be_u32([0, 0, 0, 0]), 0);
        assert_eq!(be_u32([0x12, 0x34, 0x56, 0x78]), 0x1234_5678);
        assert_eq!(be_u32([0, 0, 1, 0x80]), 0x0000_0180, "последний байт больше 127 не должен затирать старшие");
        assert_eq!(be_u32([0xff, 0xff, 0xff, 0xff]), u32::MAX);
        for last in 0..=255u8 {
            assert_eq!(be_u32([0, 0, 0, last]), u32::from(last));
        }
    }

    #[test]
    fn low_bits_cover_every_width() {
        assert_eq!(low_bits(0xdead_beef, 0), 0);
        assert_eq!(low_bits(0xdead_beef, 4), 0xf);
        assert_eq!(low_bits(0xdead_beef, 16), 0xbeef);
        assert_eq!(low_bits(0xdead_beef, 31), 0x5ead_beef);
        assert_eq!(low_bits(0xdead_beef, 32), 0xdead_beef, "32 бита это всё число");
        assert_eq!(low_bits(0xdead_beef, 40), 0xdead_beef, "больше ширины типа: всё число");
    }
}
