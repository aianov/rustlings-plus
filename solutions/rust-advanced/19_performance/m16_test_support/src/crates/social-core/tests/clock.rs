mod common;

use common::{START, network, register};
use social_core::{TRENDS_WINDOW, Visibility};

#[test]
fn post_gets_the_time_of_the_network_clock() {
    let (mut net, clock) = network();
    let alice = register(&mut net, "alice");
    let first = net
        .publish(alice, "первый", Visibility::Public)
        .expect("пост публикуется");
    clock.advance(90);
    let second = net
        .publish(alice, "второй", Visibility::Public)
        .expect("пост публикуется");
    let time = |id| net.post(id).map(|post| post.time());
    assert_eq!(time(first), Some(START));
    assert_eq!(time(second), Some(START + 90));
    assert_eq!(net.now(), START + 90);
}

#[test]
fn trend_leaves_the_window_when_the_clock_moves() {
    let (mut net, clock) = network();
    let alice = register(&mut net, "alice");
    let bob = register(&mut net, "bob");
    let post = net
        .publish(alice, "в тренды", Visibility::Public)
        .expect("пост публикуется");
    net.like(bob, post).expect("лайк проходит");
    let top = |net: &social_core::MemNetwork| net.trending(net.now(), TRENDS_WINDOW, 5).len();
    assert_eq!(top(&net), 1);
    clock.advance(TRENDS_WINDOW - 1);
    assert_eq!(top(&net), 1, "за секунду до конца суток лайк ещё в окне");
    clock.advance(1);
    assert_eq!(top(&net), 0, "ровно через сутки лайк уже не в окне");
}
