// Вычисляет степень двойки с помощью битового сдвига.
// `1 << n` равно «2 в степени n».
fn power_of_2(n: u8) -> u64 {
    1 << n
}

fn main() {
    // Здесь можно поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn you_can_assert_eq() {
        // TODO: Проверь функцию `power_of_2` на каких-нибудь значениях.
        assert_eq!();
        assert_eq!();
        assert_eq!();
        assert_eq!();
    }
}
