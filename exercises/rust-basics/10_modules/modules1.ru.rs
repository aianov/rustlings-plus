// TODO: Исправь ошибку компилятора про вызов приватной функции.
mod sausage_factory {
    // Пусть никто за пределами этого модуля этого не видит!
    fn get_secret_recipe() -> String {
        String::from("Ginger")
    }

    fn make_sausage() {
        get_secret_recipe();
        println!("колбаса!");
    }
}

fn main() {
    sausage_factory::make_sausage();
}
