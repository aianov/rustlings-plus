// TODO: Добавь недостающий тип аргумента `num` после двоеточия `:`.
fn call_me(num:) {
    for i in 0..num {
        println!("Дзинь! Звонок номер {}", i + 1);
    }
}

fn main() {
    call_me(3);
}
