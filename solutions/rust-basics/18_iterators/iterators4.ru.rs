// Представлены 3 возможных решения.

// С циклом `for` и изменяемой переменной.
fn factorial_for(num: u64) -> u64 {
    let mut result = 1;

    for x in 2..=num {
        result *= x;
    }

    result
}

// Эквивалентно `factorial_for`, но короче и без цикла `for` и
// изменяемых переменных.
fn factorial_fold(num: u64) -> u64 {
    // Случай num==0: итератор 2..=0 пуст
    //              -> возвращается начальное значение `fold`, то есть 1.
    // Случай num==1: итератор 2..=1 тоже пуст
    //              -> возвращается начальное значение 1.
    // Случай num==2: итератор 2..=2 содержит один элемент
    //              -> начальное значение 1 умножается на 2, и результат
    //                 возвращается.
    // Случай num==3: итератор 2..=3 содержит 2 элемента
    //              -> считается 1 * 2, затем результат 2 умножается на
    //                 второй элемент 3, и возвращается результат 6.
    // И так далее…
    #[allow(clippy::unnecessary_fold)]
    (2..=num).fold(1, |acc, x| acc * x)
}

// Эквивалентно `factorial_fold`, но со встроенным методом, который
// предлагает Clippy.
fn factorial_product(num: u64) -> u64 {
    (2..=num).product()
}

fn main() {
    // Здесь можно поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factorial_of_0() {
        assert_eq!(factorial_for(0), 1);
        assert_eq!(factorial_fold(0), 1);
        assert_eq!(factorial_product(0), 1);
    }

    #[test]
    fn factorial_of_1() {
        assert_eq!(factorial_for(1), 1);
        assert_eq!(factorial_fold(1), 1);
        assert_eq!(factorial_product(1), 1);
    }
    #[test]
    fn factorial_of_2() {
        assert_eq!(factorial_for(2), 2);
        assert_eq!(factorial_fold(2), 2);
        assert_eq!(factorial_product(2), 2);
    }

    #[test]
    fn factorial_of_4() {
        assert_eq!(factorial_for(4), 24);
        assert_eq!(factorial_fold(4), 24);
        assert_eq!(factorial_product(4), 24);
    }
}
