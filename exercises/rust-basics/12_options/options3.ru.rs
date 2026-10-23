#[derive(Debug)]
struct Point {
    x: i32,
    y: i32,
}

fn main() {
    let optional_point = Some(Point { x: 100, y: 200 });

    // TODO: Исправь ошибку компилятора, добавив что-нибудь в этот match.
    match optional_point {
        Some(p) => println!("Координаты: {},{}", p.x, p.y),
        _ => panic!("Нет совпадения!"),
    }

    println!("{optional_point:?}"); // Эту строку не меняй.
}
