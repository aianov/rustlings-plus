use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(n) => println!("1970-01-01 00:00:00 UTC был {} секунд назад!", n.as_secs()),
        Err(_) => panic!("SystemTime раньше UNIX EPOCH!"),
    }
}
