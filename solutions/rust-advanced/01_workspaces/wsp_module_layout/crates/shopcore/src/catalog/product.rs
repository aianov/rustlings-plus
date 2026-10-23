#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Product {
    pub sku: String,
    pub name: String,
    pub price_cents: u32,
}

impl Product {
    pub fn new(sku: &str, name: &str, price_cents: u32) -> Product {
        Product { sku: sku.to_string(), name: name.to_string(), price_cents }
    }
}
