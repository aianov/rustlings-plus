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

impl Unit for Meter {
    const SYMBOL: &'static str = "m";
    const TO_BASE: f64 = 1.0;
}

impl Unit for Kilometer {
    const SYMBOL: &'static str = "km";
    const TO_BASE: f64 = 1000.0;

    fn describe(&self, value: f64) -> String {
        format!("{value:.1}{}", Self::SYMBOL)
    }
}

impl Unit for Mile {
    const SYMBOL: &'static str = "mi";
    const TO_BASE: f64 = 1609.344;
}

pub fn convert<A: Unit, B: Unit>(value: f64, from: &A, to: &B) -> f64 {
    from.to_base(value) / to.to_base(1.0)
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
