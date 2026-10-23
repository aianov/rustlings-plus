// В базе у пользователей и постов обычные числовые id, и однажды `get_post(user_id)` молча вернул чужой
// пост. Хочется, чтобы такая ошибка не собиралась. Идея: `Id<T>` хранит число, а `T` только помечает,
// чей это id. Хранилище `Store<T>` выдаёт `Id<T>` при вставке и принимает только `Id<T>` при чтении.
// Тест проверяет не только поведение, но и то, что смешение типов не компилируется, и что `Id<T>`
// не заражает себя свойствами `T` (например, `Id<Rc<u8>>` обязан оставаться `Send`).
// TODO: Напиши `Id<T>` с `new`, `raw` и `Store<T>` с `new`, `insert`, `get`, `len`. Ids выдаются с нуля подряд.
use std::marker::PhantomData;

pub struct User;
pub struct Post;

fn main() {
    println!("{}", std::mem::size_of::<u64>());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    const SRC: &str = include_str!("gnc8.rs");

    fn compiles_with(extra: &str) -> bool {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("rustlings_probe_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).expect("не удалось создать папку");
        let file = dir.join("probe.rs");
        let code = SRC.split("#[cfg(test)]").next().unwrap_or("");
        std::fs::write(&file, format!("{code}\n{extra}\n")).expect("не удалось записать файл");
        let ok = std::process::Command::new("rustc")
            .args(["--edition", "2024", "--crate-type", "lib", "--emit=metadata", "-A", "warnings", "--out-dir"])
            .arg(&dir)
            .arg(&file)
            .output()
            .expect("не удалось запустить rustc")
            .status
            .success();
        let _ = std::fs::remove_dir_all(&dir);
        ok
    }


    #[test]
    fn store_hands_out_sequential_ids() {
        let mut posts: Store<String> = Store::new();
        let a = posts.insert("first".to_string());
        let b = posts.insert("second".to_string());
        assert_eq!((a.raw(), b.raw()), (0, 1), "ids идут с нуля подряд");
        assert_eq!(posts.len(), 2);
        assert_eq!(posts.get(Id::new(1)).map(String::as_str), Some("second"));
        assert_eq!(posts.get(Id::new(9)), None, "чужого номера нет");
    }

    #[test]
    fn id_is_as_small_as_a_number() {
        assert_eq!(std::mem::size_of::<Id<User>>(), 8, "Id хранит только число");
        assert_eq!(Id::<Post>::new(5).raw(), 5);
    }

    #[test]
    fn id_does_not_inherit_traits_of_its_marker() {
        fn is_send<T: Send>() {}
        fn is_sync<T: Sync>() {}
        is_send::<Id<Rc<u8>>>();
        is_sync::<Id<Rc<u8>>>();
    }

    #[test]
    fn user_and_post_ids_do_not_mix() {
        assert!(
            compiles_with("fn ok(s: &Store<User>, id: Id<User>) { s.get(id); }"),
            "пара Store<User> и Id<User> должна собираться"
        );
        assert!(
            !compiles_with("fn mix(s: &Store<User>, id: Id<Post>) { s.get(id); }"),
            "Id<Post> в Store<User> подставить нельзя: это и есть смысл типа"
        );
        assert!(
            !compiles_with("fn conv(id: Id<Post>) -> Id<User> { id }"),
            "и молча превратить одно в другое тоже"
        );
    }
}
