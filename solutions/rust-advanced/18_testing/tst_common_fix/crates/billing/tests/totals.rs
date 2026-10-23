mod common;

#[test]
fn subtotal_sums_price_times_quantity() {
    assert_eq!(common::coffee_invoice().subtotal(), 920);
}

#[test]
fn empty_invoice_costs_nothing() {
    let invoice = common::empty_invoice();
    assert_eq!(invoice.subtotal(), 0);
    assert_eq!(invoice.line_count(), 0);
}

#[test]
fn lines_are_counted() {
    assert_eq!(common::coffee_invoice().line_count(), 2);
}
