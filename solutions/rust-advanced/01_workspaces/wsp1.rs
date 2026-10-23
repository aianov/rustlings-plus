mod shop {
    pub const TAX_PERCENT: u32 = 20;

    pub fn with_tax(cents: u32) -> u32 {
        cents + cents * TAX_PERCENT / 100
    }

    pub mod cart {
        use super::with_tax;

        pub struct Line {
            pub price: u32,
            pub qty: u32,
        }

        pub fn total(lines: &[Line]) -> u32 {
            lines.iter().map(line_sum).sum()
        }

        pub fn total_with_tax(lines: &[Line]) -> u32 {
            with_tax(total(lines))
        }

        fn line_sum(line: &Line) -> u32 {
            line.price * line.qty
        }
    }

    pub mod report {
        use super::TAX_PERCENT;
        use super::cart;

        pub fn summary(lines: &[cart::Line]) -> String {
            format!("{} cents with {}% tax", cart::total_with_tax(lines), TAX_PERCENT)
        }

        pub fn one_line(price: u32, qty: u32) -> String {
            summary(&[cart::Line { price, qty }])
        }
    }
}

mod audit {
    use crate::shop;

    pub fn tax_is_sane() -> bool {
        shop::TAX_PERCENT < 100
    }

    pub fn first_line_total(price: u32) -> u32 {
        shop::cart::total(&[shop::cart::Line { price, qty: 1 }])
    }
}

fn main() {
    println!("{}", shop::report::one_line(250, 3));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tax_is_added_on_top() {
        assert_eq!(shop::with_tax(1000), 1200, "20% налога от 1000 даёт 1200");
    }

    #[test]
    fn cart_sums_price_times_quantity() {
        let lines = [shop::cart::Line { price: 100, qty: 3 }, shop::cart::Line { price: 50, qty: 2 }];
        assert_eq!(shop::cart::total(&lines), 400, "100*3 + 50*2");
        assert_eq!(shop::cart::total_with_tax(&lines), 480, "400 плюс 20%");
    }

    #[test]
    fn report_uses_the_cart_and_the_rate() {
        assert_eq!(shop::report::one_line(250, 3), "900 cents with 20% tax", "750 плюс 20% это 900");
    }

    #[test]
    fn audit_reaches_into_shop() {
        assert!(audit::tax_is_sane(), "налог 20% должен считаться разумным");
        assert_eq!(audit::first_line_total(70), 70, "одна позиция в одном экземпляре");
    }
}
