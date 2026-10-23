use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Money(pub i64);

impl Add for Money {
    type Output = Money;

    fn add(self, rhs: Money) -> Money {
        Money(self.0 + rhs.0)
    }
}

impl Sub for Money {
    type Output = Money;

    fn sub(self, rhs: Money) -> Money {
        Money(self.0 - rhs.0)
    }
}

impl Neg for Money {
    type Output = Money;

    fn neg(self) -> Money {
        Money(-self.0)
    }
}

impl AddAssign for Money {
    fn add_assign(&mut self, rhs: Money) {
        self.0 += rhs.0;
    }
}

impl Mul<i64> for Money {
    type Output = Money;

    fn mul(self, rhs: i64) -> Money {
        Money(self.0 * rhs)
    }
}

impl Mul<Money> for i64 {
    type Output = Money;

    fn mul(self, rhs: Money) -> Money {
        rhs * self
    }
}

impl Sum for Money {
    fn sum<I: Iterator<Item = Money>>(iter: I) -> Money {
        iter.fold(Money(0), |acc, m| acc + m)
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        write!(f, "{sign}{}.{:02}", abs / 100, abs % 100)
    }
}

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
