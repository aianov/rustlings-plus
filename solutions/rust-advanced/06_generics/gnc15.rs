pub struct New;
pub struct Paid;
pub struct Shipped;

pub struct Order<S> {
    id: u32,
    total_cents: u64,
    paid_cents: u64,
    tracking: Option<String>,
    _state: std::marker::PhantomData<S>,
}

impl<S> Order<S> {
    pub fn id(&self) -> u32 {
        self.id
    }
}

impl Order<New> {
    pub fn create(id: u32, total_cents: u64) -> Self {
        Order { id, total_cents, paid_cents: 0, tracking: None, _state: std::marker::PhantomData }
    }

    pub fn pay(self, cents: u64) -> Order<Paid> {
        Order { id: self.id, total_cents: self.total_cents, paid_cents: cents, tracking: self.tracking, _state: std::marker::PhantomData }
    }
}

impl Order<Paid> {
    pub fn ship(self) -> Order<Shipped> {
        let tracking = Some(format!("TRK-{}", self.id));
        Order { id: self.id, total_cents: self.total_cents, paid_cents: self.paid_cents, tracking, _state: std::marker::PhantomData }
    }
}

impl Order<Shipped> {
    pub fn tracking(&self) -> &str {
        self.tracking.as_deref().unwrap_or("")
    }
}

impl<S> Order<S> {
    pub fn is_fully_paid(&self) -> bool {
        self.paid_cents >= self.total_cents
    }
}

fn main() {
    let order = Order::create(1, 500).pay(500).ship();
    println!("{}", order.tracking());
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = include_str!("gnc15.rs");

    fn compiles_with(extra: &str) -> bool {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("rustlings_probe_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).expect("не удалось создать папку");
        let file = dir.join("probe.rs");
        let code = SRC.split("#[cfg(test)]").next().unwrap_or("");
        std::fs::write(&file, format!("{code}\n{extra}\n")).expect("не удалось записать файл");
        let ok = std::process::Command::new("rustc")
            .args(["--edition", "2024", "--crate-type", "lib", "--emit=metadata", "-A", "warnings", "--out-dir"])
            .arg(&dir)
            .arg(&file)
            .output()
            .expect("не удалось запустить rustc")
            .status
            .success();
        let _ = std::fs::remove_dir_all(&dir);
        ok
    }


    #[test]
    fn full_lifecycle() {
        let order = Order::create(7, 1000).pay(1000);
        assert!(order.is_fully_paid());
        let shipped = order.ship();
        assert_eq!(shipped.tracking(), "TRK-7");
        assert_eq!(shipped.id(), 7);
    }

    #[test]
    fn partial_payment_is_visible() {
        let order = Order::create(1, 1000).pay(400);
        assert!(!order.is_fully_paid(), "оплачено 400 из 1000");
    }

    #[test]
    fn cannot_ship_an_unpaid_order() {
        assert!(compiles_with("fn p(o: Order<Paid>) -> Order<Shipped> { o.ship() }"), "оплаченный заказ отправляется");
        assert!(!compiles_with("fn p(o: Order<New>) -> Order<Shipped> { o.ship() }"), "новый заказ отправить нельзя");
    }

    #[test]
    fn cannot_ship_twice_or_pay_after_shipping() {
        assert!(!compiles_with("fn p(o: Order<Shipped>) -> Order<Shipped> { o.ship() }"), "повторная отправка не должна собираться");
        assert!(!compiles_with("fn p(o: Order<Shipped>) -> Order<Paid> { o.pay(1) }"), "оплата отправленного не должна собираться");
        assert!(!compiles_with("fn p(o: Order<Paid>) -> Order<Paid> { o.pay(1) }"), "двойная оплата тоже");
    }

    #[test]
    fn tracking_only_exists_after_shipping() {
        assert!(!compiles_with("fn p(o: &Order<Paid>) -> &str { o.tracking() }"), "номер отправления есть только у Order<Shipped>");
    }
}
