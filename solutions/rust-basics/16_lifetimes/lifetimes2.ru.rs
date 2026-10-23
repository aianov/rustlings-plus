fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}

fn main() {
    let string1 = String::from("long string is long");
    // Решение 1: можно вынести `string2` из внутреннего блока, чтобы оно не
    // было уничтожено до вызова печати.
    let string2 = String::from("xyz");
    let result;
    {
        result = longest(&string1, &string2);
    }
    println!("Самая длинная строка: '{result}'");
    // `string2` уничтожается в конце функции.

    // =========================================================================

    let string1 = String::from("long string is long");
    let result;
    {
        let string2 = String::from("xyz");
        result = longest(&string1, &string2);
        // Решение 2: можно перенести печать во внутренний блок, чтобы она
        // выполнилась до уничтожения `string2`.
        println!("Самая длинная строка: '{result}'");
        // `string2` уничтожается здесь (конец внутренней области видимости).
    }
}
