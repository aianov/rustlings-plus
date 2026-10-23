mod sausage_factory {
    fn get_secret_recipe() -> String {
        String::from("Ginger")
    }

    // Добавили `pub` перед `fn`, чтобы функция стала доступна за пределами модуля.
    pub fn make_sausage() {
        get_secret_recipe();
        println!("колбаса!");
    }
}

fn main() {
    sausage_factory::make_sausage();
}
