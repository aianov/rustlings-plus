// Редактор списка покупок делает пачку правок, и если на середине что-то пошло не так (ошибка, паника,
// ранний `return`), список должен остаться как был. Писать откат вручную в каждой ветке ненадёжно:
// кто-нибудь добавит ещё один `return` и забудет. Лучше охранник (RAII): создаётся при начале, при
// уничтожении откатывает, если до `commit` дело не дошло. `commit` забирает охранник по значению
// и подтверждает правки.
// TODO: Реализуй `begin`, `push`, `remove_first`, `view`, `commit` и `Drop` для `Transaction`.

pub struct Transaction<'a> {
    target: &'a mut Vec<String>,
    snapshot: Vec<String>,
    committed: bool,
}

impl<'a> Transaction<'a> {
    pub fn begin(target: &'a mut Vec<String>) -> Transaction<'a> {
        todo!("запомнить исходное состояние")
    }

    pub fn push(&mut self, item: &str) {
        todo!()
    }

    pub fn remove_first(&mut self) -> Option<String> {
        todo!()
    }

    pub fn view(&self) -> &[String] {
        todo!()
    }

    pub fn commit(mut self) {
        todo!()
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        todo!("откатить, если не подтверждено")
    }
}

fn main() {
    let mut list = vec!["milk".to_string()];
    let mut tx = Transaction::begin(&mut list);
    tx.push("eggs");
    tx.commit();
    println!("{list:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    fn list(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn commit_keeps_the_changes() {
        let mut shopping = list(&["milk"]);
        let mut tx = Transaction::begin(&mut shopping);
        tx.push("eggs");
        assert_eq!(tx.view(), ["milk", "eggs"], "внутри транзакции правки видны сразу");
        tx.commit();
        assert_eq!(shopping, list(&["milk", "eggs"]));
    }

    #[test]
    fn dropping_without_commit_rolls_back() {
        let mut shopping = list(&["milk", "bread"]);
        {
            let mut tx = Transaction::begin(&mut shopping);
            tx.push("eggs");
            assert_eq!(tx.remove_first().as_deref(), Some("milk"));
            assert_eq!(tx.view(), ["bread", "eggs"]);
        }
        assert_eq!(shopping, list(&["milk", "bread"]), "без commit список возвращается к исходному");
    }

    fn risky(items: &mut Vec<String>, fail_at: usize) -> Result<(), String> {
        let mut tx = Transaction::begin(items);
        for (i, name) in ["a", "b", "c"].iter().enumerate() {
            if i == fail_at {
                return Err(format!("failed on {name}"));
            }
            tx.push(name);
        }
        tx.commit();
        Ok(())
    }

    #[test]
    fn early_return_rolls_back_too() {
        let mut items = list(&["x"]);
        assert!(risky(&mut items, 2).is_err());
        assert_eq!(items, list(&["x"]), "ранний return должен откатить без единой строки ручного отката");
        assert!(risky(&mut items, 9).is_ok());
        assert_eq!(items, list(&["x", "a", "b", "c"]));
    }

    #[test]
    fn panic_rolls_back() {
        let mut items = list(&["x"]);
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut tx = Transaction::begin(&mut items);
            tx.push("y");
            panic!("boom");
        }));
        assert!(result.is_err());
        assert_eq!(items, list(&["x"]), "при панике деструктор тоже вызывается и откатывает");
    }

    #[test]
    fn target_is_usable_after_each_transaction() {
        let mut items = list(&[]);
        Transaction::begin(&mut items).push("lost");
        assert!(items.is_empty(), "транзакцию уничтожили без commit");
        let mut tx = Transaction::begin(&mut items);
        tx.push("kept");
        tx.commit();
        items.push("direct".to_string());
        assert_eq!(items, list(&["kept", "direct"]));
    }
}
