// Единицы длины описаны трейтом с двумя константами (символ и множитель к метру) и двумя готовыми
// методами. Константы принадлежат типу, а не значению, поэтому доступны как `Unit::SYMBOL` без
// объекта. Методы по умолчанию можно переопределять там, где у типа своя логика.
// Метр: символ m, множитель 1. Километр: km, 1000. Миля: mi, 1609.344.
// По умолчанию `describe(1.5)` это `1.5m` (число и символ). Километры показывают одну цифру после запятой
// (`2.0km`), остальные число как есть. `to_base` по умолчанию умножает на множитель.
// TODO: Реализуй `Unit` для Meter, Kilometer, Mile и допиши `convert`.

pub trait Unit {
    const SYMBOL: &'static str;
    const TO_BASE: f64;

    fn describe(&self, value: f64) -> String {
        format!("{value}{}", Self::SYMBOL)
    }

    fn to_base(&self, value: f64) -> f64 {
        value * Self::TO_BASE
    }
}

pub struct Meter;
pub struct Kilometer;
pub struct Mile;

pub fn convert<A: Unit, B: Unit>(value: f64, from: &A, to: &B) -> f64 {
    todo!("перевод через метры")
}

fn main() {
    println!("{}", Meter.describe(1.5));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_belong_to_types() {
        assert_eq!(Meter::SYMBOL, "m");
        assert_eq!(<Kilometer as Unit>::SYMBOL, "km");
        assert_eq!(Mile::TO_BASE, 1609.344);
    }

    #[test]
    fn default_and_overridden_describe() {
        assert_eq!(Meter.describe(1.5), "1.5m", "умолчание из трейта");
        assert_eq!(Mile.describe(2.0), "2mi", "у мили тоже умолчание");
        assert_eq!(Kilometer.describe(2.0), "2.0km", "километры переопределили форму записи");
        assert_eq!(Kilometer.describe(1.26), "1.3km");
    }

    #[test]
    fn default_to_base() {
        assert_eq!(Kilometer.to_base(2.0), 2000.0);
        assert!((Mile.to_base(1.0) - 1609.344).abs() < 1e-9);
        assert_eq!(Meter.to_base(7.0), 7.0);
    }

    #[test]
    fn conversion_between_units() {
        assert_eq!(convert(3.0, &Kilometer, &Meter), 3000.0);
        assert!((convert(1.0, &Mile, &Kilometer) - 1.609344).abs() < 1e-9);
        assert!((convert(1609.344, &Meter, &Mile) - 1.0).abs() < 1e-9);
    }
}
