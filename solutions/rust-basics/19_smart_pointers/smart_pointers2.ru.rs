// В этом упражнении мы выражаем идею нескольких владельцев через тип
// `Rc<T>`. Это модель нашей Солнечной системы: есть тип `Sun` и несколько
// `Planet`. Планеты владеют Солнцем, показывая, что они вращаются вокруг
// него.

use std::rc::Rc;

#[derive(Debug)]
struct Sun;

#[derive(Debug)]
enum Planet {
    Mercury(Rc<Sun>),
    Venus(Rc<Sun>),
    Earth(Rc<Sun>),
    Mars(Rc<Sun>),
    Jupiter(Rc<Sun>),
    Saturn(Rc<Sun>),
    Uranus(Rc<Sun>),
    Neptune(Rc<Sun>),
}

impl Planet {
    fn details(&self) {
        println!("Привет от {self:?}!");
    }
}

fn main() {
    // Здесь можно поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rc1() {
        let sun = Rc::new(Sun);
        println!("счётчик ссылок = {}", Rc::strong_count(&sun)); // 1 ссылка

        let mercury = Planet::Mercury(Rc::clone(&sun));
        println!("счётчик ссылок = {}", Rc::strong_count(&sun)); // 2 ссылки
        mercury.details();

        let venus = Planet::Venus(Rc::clone(&sun));
        println!("счётчик ссылок = {}", Rc::strong_count(&sun)); // 3 ссылки
        venus.details();

        let earth = Planet::Earth(Rc::clone(&sun));
        println!("счётчик ссылок = {}", Rc::strong_count(&sun)); // 4 ссылки
        earth.details();

        let mars = Planet::Mars(Rc::clone(&sun));
        println!("счётчик ссылок = {}", Rc::strong_count(&sun)); // 5 ссылок
        mars.details();

        let jupiter = Planet::Jupiter(Rc::clone(&sun));
        println!("счётчик ссылок = {}", Rc::strong_count(&sun)); // 6 ссылок
        jupiter.details();

        let saturn = Planet::Saturn(Rc::clone(&sun));
        println!("счётчик ссылок = {}", Rc::strong_count(&sun)); // 7 ссылок
        saturn.details();

        let uranus = Planet::Uranus(Rc::clone(&sun));
        println!("счётчик ссылок = {}", Rc::strong_count(&sun)); // 8 ссылок
        uranus.details();

        let neptune = Planet::Neptune(Rc::clone(&sun));
        println!("счётчик ссылок = {}", Rc::strong_count(&sun)); // 9 ссылок
        neptune.details();

        assert_eq!(Rc::strong_count(&sun), 9);

        drop(neptune);
        println!("счётчик ссылок = {}", Rc::strong_count(&sun)); // 8 ссылок

        drop(uranus);
        println!("счётчик ссылок = {}", Rc::strong_count(&sun)); // 7 ссылок

        drop(saturn);
        println!("счётчик ссылок = {}", Rc::strong_count(&sun)); // 6 ссылок

        drop(jupiter);
        println!("счётчик ссылок = {}", Rc::strong_count(&sun)); // 5 ссылок

        drop(mars);
        println!("счётчик ссылок = {}", Rc::strong_count(&sun)); // 4 ссылки

        drop(earth);
        println!("счётчик ссылок = {}", Rc::strong_count(&sun)); // 3 ссылки

        drop(venus);
        println!("счётчик ссылок = {}", Rc::strong_count(&sun)); // 2 ссылки

        drop(mercury);
        println!("счётчик ссылок = {}", Rc::strong_count(&sun)); // 1 ссылка

        assert_eq!(Rc::strong_count(&sun), 1);
    }
}
