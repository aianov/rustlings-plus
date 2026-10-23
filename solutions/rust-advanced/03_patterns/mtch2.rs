use std::collections::HashMap;

pub struct User {
    pub active: bool,
    pub email: Option<String>,
    pub email_verified: bool,
}

pub fn mailing_address(users: &HashMap<u32, User>, id: u32) -> Option<&str> {
    if let Some(user) = users.get(&id)
        && user.active
        && user.email_verified
        && let Some(email) = &user.email
    {
        Some(email)
    } else {
        None
    }
}

#[derive(Clone)]
pub enum Task {
    Print(String),
    Group(Vec<Task>),
    Repeat(u32, Box<Task>),
}

pub fn run(root: Task) -> Vec<String> {
    let mut printed = Vec::new();
    let mut stack = vec![root];
    while let Some(task) = stack.pop() {
        match task {
            Task::Print(text) => printed.push(text),
            Task::Group(children) => stack.extend(children.into_iter().rev()),
            Task::Repeat(times, inner) => {
                for _ in 0..times {
                    stack.push((*inner).clone());
                }
            }
        }
    }
    printed
}

fn main() {
    println!("{:?}", run(Task::Print("hello".into())));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn users() -> HashMap<u32, User> {
        let mut m = HashMap::new();
        m.insert(1, User { active: true, email: Some("ann@example.com".into()), email_verified: true });
        m.insert(2, User { active: false, email: Some("bob@example.com".into()), email_verified: true });
        m.insert(3, User { active: true, email: Some("cat@example.com".into()), email_verified: false });
        m.insert(4, User { active: true, email: None, email_verified: true });
        m
    }

    #[test]
    fn address_of_a_good_user() {
        assert_eq!(mailing_address(&users(), 1), Some("ann@example.com"));
    }

    #[test]
    fn every_condition_matters() {
        let u = users();
        assert_eq!(mailing_address(&u, 2), None, "неактивному не пишем");
        assert_eq!(mailing_address(&u, 3), None, "неподтверждённый адрес не годится");
        assert_eq!(mailing_address(&u, 4), None, "адреса нет совсем");
        assert_eq!(mailing_address(&u, 99), None, "такого пользователя нет");
    }

    fn print(s: &str) -> Task {
        Task::Print(s.to_string())
    }

    #[test]
    fn plain_print() {
        assert_eq!(run(print("a")), vec!["a"]);
    }

    #[test]
    fn group_runs_top_to_bottom() {
        let g = Task::Group(vec![print("a"), print("b"), print("c")]);
        assert_eq!(run(g), vec!["a", "b", "c"], "задания группы идут в записанном порядке, а не с конца");
    }

    #[test]
    fn nested_groups_keep_order() {
        let g = Task::Group(vec![
            print("1"),
            Task::Group(vec![print("2"), Task::Group(vec![print("3"), print("4")]), print("5")]),
            print("6"),
        ]);
        assert_eq!(run(g), vec!["1", "2", "3", "4", "5", "6"], "вложенная группа выполняется целиком на своём месте");
    }

    #[test]
    fn repeat_repeats_whole_subtree() {
        let g = Task::Group(vec![
            print("start"),
            Task::Repeat(3, Box::new(Task::Group(vec![print("x"), print("y")]))),
            print("end"),
        ]);
        assert_eq!(run(g), vec!["start", "x", "y", "x", "y", "x", "y", "end"]);
        assert!(run(Task::Repeat(0, Box::new(print("never")))).is_empty(), "ноль повторов это ничего");
    }

    #[test]
    fn long_chain_does_not_blow_the_stack() {
        let mut task = print("deep");
        for _ in 0..200_000 {
            task = Task::Group(vec![task]);
        }
        let out = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || run(task))
            .expect("поток не запустился")
            .join()
            .expect("обход упал");
        assert_eq!(out, vec!["deep"], "если здесь аварийный выход со stack overflow, обход рекурсивный: нужен явный стек");
    }

    #[test]
    fn loop_is_explicit() {
        let prod = include_str!("mtch2.rs").split("#[cfg(test)]").next().expect("файл не пуст");
        assert!(prod.contains("while let"), "обход должен быть циклом `while let` по стеку");
        assert!(prod.contains("&& let "), "в адресе нужна цепочка `if let ... && ... && let ...`");
    }
}
