#[derive(Debug)]
struct Point {
    x: i32,
    y: i32,
}

fn main() {
    let optional_point = Some(Point { x: 100, y: 200 });

    // Решение 1: сопоставляем `Option` (не `&Option`), но не перемещаем
    // значение из варианта `Some`.
    match optional_point {
        Some(ref p) => println!("Координаты: {},{}", p.x, p.y),
        //   ^^^ добавлено
        _ => panic!("Нет совпадения!"),
    }

    // Решение 2: сопоставляем ссылку (`&Option`), добавив `&` перед
    // `optional_point`.
    match &optional_point {
        //^ добавлено
        Some(p) => println!("Координаты: {},{}", p.x, p.y),
        _ => panic!("Нет совпадения!"),
    }

    println!("{optional_point:?}");
}
