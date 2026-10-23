use shopcore::{
    Account, Invoice, Product, Stock, StockError, add_vat, format_cents, points_for, welcome_points,
};

#[test]
fn stock_tracks_quantities() {
    let mut stock = Stock::new();
    stock.add(Product::new("tea", "Green tea", 450), 10);
    assert_eq!(stock.quantity("tea"), 10);
    assert_eq!(stock.take("tea", 4).map(|p| p.name), Ok("Green tea".to_string()));
    assert_eq!(stock.quantity("tea"), 6);
}

#[test]
fn stock_refuses_what_it_does_not_have() {
    let mut stock = Stock::new();
    stock.add(Product::new("tea", "Green tea", 450), 2);
    assert_eq!(stock.take("tea", 3), Err(StockError::NotEnough { sku: "tea".to_string(), left: 2 }));
    assert_eq!(stock.take("rum", 1), Err(StockError::Unknown("rum".to_string())));
}

#[test]
fn money_and_tax() {
    assert_eq!(format_cents(1205), "12.05");
    assert_eq!(add_vat(1000), 1200);
}

#[test]
fn invoice_renders_lines_and_total() {
    let mut invoice = Invoice::new();
    invoice.add(Product::new("tea", "Green tea", 450), 2);
    invoice.add(Product::new("cup", "Cup", 300), 1);
    assert_eq!(invoice.total(), 1440);
    assert_eq!(invoice.render(), "Green tea x2: 9.00\nCup x1: 3.00\ntotal: 14.40");
}

#[test]
fn loyalty_points() {
    let ann = Account::new(1, "ann");
    assert_eq!(points_for(1999), 19);
    assert_eq!(welcome_points(&ann, 500), "ann earns 5 points");
}
