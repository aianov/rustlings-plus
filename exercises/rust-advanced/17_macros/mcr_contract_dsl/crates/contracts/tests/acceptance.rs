use contracts::{find_duplicates, Route};
use std::collections::HashSet;

pub struct Ping;
pub struct Pong;
pub struct Page<T>(pub Vec<T>);
pub struct Item;

contracts::service! {
    /// Тестовый сервис.
    pub enum DemoMethod {
        Ping = "demo.ping" (Ping) -> Pong,
        List = "demo.list" (Page<Item>) -> Page<Item>,
        Raw = "demo.raw" ((u8, u8)) -> Vec<String>,
    }
}

contracts::service! {
    enum Single {
        Only = "single.only" (Ping) -> Pong
    }
}

contracts::service! {
    enum Broken {
        A = "same" (Ping) -> Pong,
        B = "other" (Ping) -> Pong,
        C = "same" (Ping) -> Pong,
        D = "same" (Ping) -> Pong,
        E = "other" (Ping) -> Pong,
    }
}

#[test]
fn variants_are_listed_in_the_written_order() {
    assert_eq!(DemoMethod::ALL, [DemoMethod::Ping, DemoMethod::List, DemoMethod::Raw]);
    assert_eq!(Single::ALL, [Single::Only]);
}

#[test]
fn names_and_types_are_available_per_method() {
    assert_eq!(DemoMethod::Ping.name(), "demo.ping");
    assert_eq!(DemoMethod::Ping.request_type(), "Ping");
    assert_eq!(DemoMethod::Ping.response_type(), "Pong");
    assert_eq!(DemoMethod::List.request_type().replace(' ', ""), "Page<Item>");
    assert_eq!(DemoMethod::Raw.response_type().replace(' ', ""), "Vec<String>");
}

#[test]
fn from_name_is_the_inverse_of_name() {
    for method in DemoMethod::ALL {
        assert_eq!(DemoMethod::from_name(method.name()), Some(*method));
    }
    assert_eq!(DemoMethod::from_name("demo.nothing"), None);
    assert_eq!(DemoMethod::from_name("DEMO.PING"), None, "имена регистрозависимые");
}

#[test]
fn routes_are_a_constant_table() {
    const TABLE: &[Route] = DemoMethod::ROUTES;
    assert_eq!(TABLE.len(), 3);
    assert_eq!(TABLE[0], Route { name: "demo.ping", request: "Ping", response: "Pong" });
    assert_eq!(TABLE[2].name, "demo.raw");
    let names: Vec<_> = DemoMethod::ALL.iter().map(|m| m.name()).collect();
    let from_routes: Vec<_> = TABLE.iter().map(|r| r.name).collect();
    assert_eq!(names, from_routes, "таблица и enum описывают одни и те же методы в одном порядке");
}

#[test]
fn good_services_have_no_duplicates() {
    assert!(DemoMethod::duplicates().is_empty());
    assert!(Single::duplicates().is_empty());
}

#[test]
fn duplicate_names_are_reported_once_each() {
    assert_eq!(Broken::duplicates(), vec!["same", "other"]);
    assert_eq!(find_duplicates(&["a", "b", "a", "a"]), vec!["a"]);
}

#[test]
fn derives_and_attributes_come_through() {
    let set: HashSet<DemoMethod> = DemoMethod::ALL.iter().copied().collect();
    assert_eq!(set.len(), 3);
    let copy = DemoMethod::Ping;
    let again = copy;
    assert_eq!(copy, again);
    assert_eq!(format!("{:?}", DemoMethod::List), "List");
}
