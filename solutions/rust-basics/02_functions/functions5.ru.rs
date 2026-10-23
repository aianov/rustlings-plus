fn square(num: i32) -> i32 {
    // Убрали точку с запятой `;` в конце строки ниже, чтобы результат вернулся неявно.
    num * num
}

fn main() {
    let answer = square(3);
    println!("Квадрат числа 3 равен {answer}");
}
