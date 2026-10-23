// Одно и то же значение можно держать тремя способами, и у каждого своя цена. Ссылка `&dyn Greeter`
// ничего не владеет, `Box<dyn Greeter>` владеет единолично, `Arc<dyn Greeter>` делится владением.
// Во всех трёх указатель «толстый»: он несёт и адрес данных, и адрес таблицы методов конкретного типа.
// TODO: Допиши четыре функции. Реализации `Greeter` готовы.

use std::sync::Arc;

pub trait Greeter {
    fn greet(&self, name: &str) -> String;
}

pub struct Loud;
pub struct Polite {
    pub title: String,
}

impl Greeter for Loud {
    fn greet(&self, name: &str) -> String {
        format!("HELLO {}!", name.to_uppercase())
    }
}

impl Greeter for Polite {
    fn greet(&self, name: &str) -> String {
        format!("Good day, {} {name}", self.title)
    }
}

pub fn greet_ref(g: &dyn Greeter, name: &str) -> String {
    todo!("поприветствовать через ссылку на трейт-объект")
}

pub fn into_boxed(g: impl Greeter + 'static) -> Box<dyn Greeter> {
    todo!("упаковать любой Greeter в Box<dyn Greeter>")
}

pub fn share(g: Arc<dyn Greeter>, copies: usize) -> Vec<Arc<dyn Greeter>> {
    todo!("вернуть copies указателей на один и тот же объект")
}

pub fn pointer_sizes() -> (usize, usize, usize) {
    todo!("размеры &dyn Greeter, Box<dyn Greeter> и обычной ссылки &Loud")
}

fn main() {
    println!("{}", greet_ref(&Loud, "ann"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_dispatches_dynamically() {
        let polite = Polite { title: "Dr".into() };
        let all: [&dyn Greeter; 2] = [&Loud, &polite];
        let got: Vec<String> = all.iter().map(|g| greet_ref(*g, "ann")).collect();
        assert_eq!(got, vec!["HELLO ANN!", "Good day, Dr ann"]);
    }

    #[test]
    fn boxing_keeps_behaviour() {
        let b = into_boxed(Polite { title: "Mr".into() });
        assert_eq!(b.greet("bob"), "Good day, Mr bob");
        assert_eq!(into_boxed(Loud).greet("x"), "HELLO X!");
    }

    #[test]
    fn sharing_points_at_one_object() {
        let g: Arc<dyn Greeter> = Arc::new(Loud);
        let copies = share(Arc::clone(&g), 3);
        assert_eq!(copies.len(), 3);
        assert!(copies.iter().all(|c| Arc::ptr_eq(c, &g)), "все копии должны указывать на один и тот же объект");
        assert_eq!(Arc::strong_count(&g), 4, "владельцев четыре: исходный и три копии");
        drop(copies);
        assert_eq!(Arc::strong_count(&g), 1);
    }

    #[test]
    fn fat_pointers_are_two_words() {
        let word = std::mem::size_of::<usize>();
        assert_eq!(pointer_sizes(), (2 * word, 2 * word, word), "ссылка на трейт-объект несёт адрес данных и адрес vtable, обычная ссылка только адрес");
    }
}
