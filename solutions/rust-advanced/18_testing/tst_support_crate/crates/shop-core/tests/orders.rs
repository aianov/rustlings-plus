use shop_core::Orders;
use test_support::{FakeClock, SeqIds};

#[test]
fn order_gets_predictable_id_and_time() {
    let orders = Orders::new(FakeClock::new(1_000), SeqIds::starting_at(7));
    let order = orders.place("кофе");
    assert_eq!((order.id, order.placed_at), (7, 1_000));
}

#[test]
fn older_than_uses_the_fake_clock() {
    let clock = FakeClock::new(0);
    let orders = Orders::new(clock.clone(), SeqIds::starting_at(1));
    orders.place("чай");
    clock.advance(100);
    orders.place("кекс");
    clock.advance(50);
    assert_eq!(orders.older_than(120), vec![1], "старше 120 секунд только первый заказ");
    assert!(orders.older_than(1_000).is_empty());
}

#[test]
fn unique_names_do_not_repeat() {
    assert_ne!(test_support::unique_name("shop"), test_support::unique_name("shop"));
}
