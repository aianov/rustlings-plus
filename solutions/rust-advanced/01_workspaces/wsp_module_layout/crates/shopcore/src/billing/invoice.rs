use super::money::format_cents;
use super::tax::add_vat;
use crate::catalog::product::Product;

#[derive(Debug, Default)]
pub struct Invoice {
    lines: Vec<(Product, u32)>,
}

impl Invoice {
    pub fn new() -> Invoice {
        Invoice::default()
    }

    pub fn add(&mut self, product: Product, qty: u32) {
        self.lines.push((product, qty));
    }

    pub fn total(&self) -> u32 {
        add_vat(self.lines.iter().map(|(p, q)| p.price_cents * q).sum())
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for (product, qty) in &self.lines {
            out.push_str(&format!("{} x{}: {}\n", product.name, qty, format_cents(product.price_cents * qty)));
        }
        out.push_str(&format!("total: {}", format_cents(self.total())));
        out
    }
}
