macro_rules! my_macro {
    () => {
        println!("Зацени мой макрос!");
    };
}

fn main() {
    my_macro!();
    //      ^
}
