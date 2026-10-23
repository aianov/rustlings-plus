use billing::Invoice;

fn coffee_invoice() -> Invoice {
    let mut invoice = Invoice::new();
    invoice.add("кофе", 350, 2).add("круассан", 220, 1);
    invoice
}

fn empty_invoice() -> Invoice {
    Invoice::new()
}

fn with_discount(percent: u8) -> Invoice {
    let mut invoice = coffee_invoice();
    invoice.discount(percent);
    invoice
}
