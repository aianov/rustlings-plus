#![allow(dead_code)]
use billing::Invoice;

pub fn coffee_invoice() -> Invoice {
    let mut invoice = Invoice::new();
    invoice.add("кофе", 350, 2).add("круассан", 220, 1);
    invoice
}

pub fn empty_invoice() -> Invoice {
    Invoice::new()
}

pub fn with_discount(percent: u8) -> Invoice {
    let mut invoice = coffee_invoice();
    invoice.discount(percent);
    invoice
}
