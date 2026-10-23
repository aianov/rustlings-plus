fn call_me(num: u8) {
    for i in 0..num {
        println!("Дзинь! Звонок номер {}", i + 1);
    }
}

fn main() {
    // `call_me` ожидает аргумент.
    call_me(5);
}
