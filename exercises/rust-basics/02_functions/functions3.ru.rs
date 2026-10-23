fn call_me(num: u8) {
    for i in 0..num {
        println!("Дзинь! Звонок номер {}", i + 1);
    }
}

fn main() {
    // TODO: Исправь вызов функции.
    call_me();
}
