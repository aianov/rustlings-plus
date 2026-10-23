pub mod catalog {
    pub mod product {
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
    }

    pub mod stock {
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
    }
}

pub mod billing {
    pub mod money {
        pub fn format_cents(cents: u32) -> String {
            format!("{}.{:02}", cents / 100, cents % 100)
        }
    }

    pub mod tax {
        const VAT_PERCENT: u32 = 20;

        pub fn add_vat(cents: u32) -> u32 {
            cents + cents * VAT_PERCENT / 100
        }
    }

    pub mod invoice {
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
    }
}

pub mod customers {
    pub mod account {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct Account {
            pub id: u64,
            pub name: String,
        }

        impl Account {
            pub fn new(id: u64, name: &str) -> Account {
                Account { id, name: name.to_string() }
            }
        }
    }

    pub mod loyalty {
        use super::account::Account;

        pub fn points_for(cents: u32) -> u32 {
            cents / 100
        }

        pub fn welcome_points(account: &Account, cents: u32) -> String {
            format!("{} earns {} points", account.name, points_for(cents))
        }
    }
}
