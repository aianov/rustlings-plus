use std::collections::HashMap;

use super::product::Product;

#[derive(Debug, PartialEq, Eq)]
pub enum StockError {
    Unknown(String),
    NotEnough { sku: String, left: u32 },
}

#[derive(Debug, Default)]
pub struct Stock {
    items: HashMap<String, (Product, u32)>,
}

impl Stock {
    pub fn new() -> Stock {
        Stock::default()
    }

    pub fn add(&mut self, product: Product, qty: u32) {
        let entry = self.items.entry(product.sku.clone()).or_insert((product, 0));
        entry.1 += qty;
    }

    pub fn quantity(&self, sku: &str) -> u32 {
        self.items.get(sku).map_or(0, |(_, q)| *q)
    }

    pub fn take(&mut self, sku: &str, qty: u32) -> Result<Product, StockError> {
        let (product, left) = self
            .items
            .get_mut(sku)
            .ok_or_else(|| StockError::Unknown(sku.to_string()))?;
        if *left < qty {
            return Err(StockError::NotEnough { sku: sku.to_string(), left: *left });
        }
        *left -= qty;
        Ok(product.clone())
    }
}
