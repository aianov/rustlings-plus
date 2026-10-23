// Деньги в копейках и операции над ними. Складывать, вычитать, менять знак, умножать на количество
// (в обоих порядках: `price * 3` и `3 * price`), накапливать через `+=` и суммировать итератор.
// Вычитание может дать отрицательные деньги: это долг, и он допустим. Умножение на количество не должно
// молча переполняться: используй обычный `*`, в отладочной сборке он паникует, этого достаточно.
// Печать (`Display`): `12.34`, для отрицательных `-0.05`, для нуля `0.00`.
// TODO: Реализуй операторы и `Display` для `Money` через нужные трейты из `std::ops`, `std::iter::Sum` и `fmt`.
use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Money(pub i64);

fn main() {
    println!("{}", Money(1234) + Money(66));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_sub_neg() {
        assert_eq!(Money(500) + Money(250), Money(750));
        assert_eq!(Money(500) - Money(750), Money(-250), "долг допустим");
        assert_eq!(-Money(300), Money(-300));
        assert_eq!(-(-Money(5)), Money(5));
    }

    #[test]
    fn add_assign_accumulates() {
        let mut wallet = Money(0);
        for cents in [100, 250, 5] {
            wallet += Money(cents);
        }
        assert_eq!(wallet, Money(355));
    }

    #[test]
    fn multiplication_both_ways() {
        assert_eq!(Money(199) * 3, Money(597));
        assert_eq!(3 * Money(199), Money(597), "количество слева тоже работает");
        let none = 0;
        assert_eq!(Money(100) * none, Money(0));
    }

    #[test]
    fn sum_of_an_iterator() {
        let prices = [Money(100), Money(250), Money(-50)];
        assert_eq!(prices.iter().copied().sum::<Money>(), Money(300));
        assert_eq!(Vec::<Money>::new().into_iter().sum::<Money>(), Money(0), "сумма пустого набора это ноль");
    }

    #[test]
    fn display_formats_cents() {
        assert_eq!(Money(1234).to_string(), "12.34");
        assert_eq!(Money(5).to_string(), "0.05");
        assert_eq!(Money(0).to_string(), "0.00");
        assert_eq!(Money(-5).to_string(), "-0.05", "у отрицательной суммы знак перед нулём");
        assert_eq!(Money(-1234).to_string(), "-12.34");
    }
}
