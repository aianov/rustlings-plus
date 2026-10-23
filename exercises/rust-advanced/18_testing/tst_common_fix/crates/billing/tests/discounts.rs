mod common;

#[test]
fn ten_percent_off() {
    assert_eq!(common::with_discount(10).total(), 828);
}

#[test]
fn discount_is_capped_at_hundred() {
    assert_eq!(common::with_discount(250).total(), 0);
}

#[test]
fn zero_discount_changes_nothing() {
    let invoice = common::with_discount(0);
    assert_eq!(invoice.total(), invoice.subtotal());
}
