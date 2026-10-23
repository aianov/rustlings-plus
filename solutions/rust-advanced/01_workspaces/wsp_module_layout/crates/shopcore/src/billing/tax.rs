const VAT_PERCENT: u32 = 20;

pub fn add_vat(cents: u32) -> u32 {
    cents + cents * VAT_PERCENT / 100
}
