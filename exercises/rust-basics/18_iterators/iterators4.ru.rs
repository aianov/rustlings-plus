fn factorial(num: u64) -> u64 {
    // TODO: Допиши функцию так, чтобы она возвращала факториал `num`, который
    // определяется как `1 * 2 * 3 * … * num`.
    // https://en.wikipedia.org/wiki/Factorial
    //
    // Не используй:
    // - ранний возврат (явный `return`)
    // Постарайся не использовать:
    // - циклы в императивном стиле (for/while)
    // - дополнительные переменные
    // Для дополнительной сложности не используй:
    // - рекурсию
}

fn main() {
    // Здесь можно поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factorial_of_0() {
        assert_eq!(factorial(0), 1);
    }

    #[test]
    fn factorial_of_1() {
        assert_eq!(factorial(1), 1);
    }
    #[test]
    fn factorial_of_2() {
        assert_eq!(factorial(2), 2);
    }

    #[test]
    fn factorial_of_4() {
        assert_eq!(factorial(4), 24);
    }
}
