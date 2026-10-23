// Магазин применяет скидки трёх видов: процент от цены, фиксированная сумма и «бесплатно при покупке
// от порога». Список доступных скидок собирается при запуске и состоит из значений разных типов, поэтому
// лежит как `Vec<Box<dyn Discount>>`. Кассе нужны две вещи: лучшая цена среди скидок (каждая применяется
// отдельно к исходной цене, скидки не складываются) и подписи для чека.
// Цены в копейках. Подписи: `10% off`, `300 off`, `free over 500`.
// TODO: Реализуй трейт `Discount` для всех трёх типов и допиши обе функции.

pub trait Discount {
    fn apply(&self, price: u64) -> u64;
    fn label(&self) -> String;
}

pub struct Percent(pub u8);
pub struct Fixed(pub u64);
pub struct FreeOver {
    pub threshold: u64,
}

pub fn best_price(discounts: &[Box<dyn Discount>], price: u64) -> u64 {
    todo!("минимум среди цен после каждой скидки; без скидок цена не меняется")
}

pub fn labels(discounts: &[Box<dyn Discount>]) -> Vec<String> {
    todo!("подписи в том же порядке")
}

fn main() {
    let ds: Vec<Box<dyn Discount>> = vec![Box::new(Percent(10))];
    println!("{}", best_price(&ds, 1000));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<Box<dyn Discount>> {
        vec![Box::new(Percent(10)), Box::new(Fixed(300)), Box::new(FreeOver { threshold: 500 })]
    }

    #[test]
    fn percent_discount_is_floored_on_the_discount_not_the_price() {
        assert_eq!(Percent(33).apply(101), 68, "скидка 33 от 101 это 33 (дробная часть отбрасывается), остаётся 68");
    }

    #[test]
    fn each_discount_alone() {
        assert_eq!(Percent(10).apply(1000), 900);
        assert_eq!(Percent(33).apply(100), 67, "проценты считаются от цены, дробная часть отбрасывается у скидки");
        assert_eq!(Percent(150).apply(1000), 0, "больше ста процентов это бесплатно, а не отрицательная цена");
        assert_eq!(Fixed(300).apply(1000), 700);
        assert_eq!(Fixed(300).apply(200), 0, "фиксированная скидка не уводит цену ниже нуля");
        assert_eq!(FreeOver { threshold: 500 }.apply(500), 0, "порог включительно");
        assert_eq!(FreeOver { threshold: 500 }.apply(499), 499);
    }

    #[test]
    fn best_of_all() {
        assert_eq!(best_price(&all(), 1000), 0, "при покупке от 500 лучшая скидка это бесплатно");
        assert_eq!(best_price(&all(), 400), 100, "при 400 лучшая это минус 300");
        assert_eq!(best_price(&all(), 100), 0, "скидка 300 перекрывает цену целиком");
    }

    #[test]
    fn discounts_do_not_stack() {
        let ds: Vec<Box<dyn Discount>> = vec![Box::new(Percent(50)), Box::new(Percent(50))];
        assert_eq!(best_price(&ds, 1000), 500, "каждая скидка применяется к исходной цене, а не друг к другу");
    }

    #[test]
    fn no_discounts_no_change() {
        assert_eq!(best_price(&[], 777), 777);
    }

    #[test]
    fn labels_for_the_receipt() {
        assert_eq!(labels(&all()), vec!["10% off", "300 off", "free over 500"]);
        assert!(labels(&[]).is_empty());
    }
}
