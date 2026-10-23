// Дерево папок: у каждой папки есть дети и родитель. Дети принадлежат родителю (удалили папку, исчезли
// вложенные), а родитель детям не принадлежит, он лишь известен им. Если и вниз, и вверх использовать `Rc`,
// получится цикл, и ничто никогда не освободится. Поэтому ссылки вниз
// сильные (`Rc`), а вверх слабые (`Weak`). Слабая ссылка не держит папку живой: `upgrade` вернёт `Some`,
// только если родитель ещё существует.
// TODO: Реализуй `Node` (`new`, `add_child`, `parent`, `path`, `depth`, `child_names`).
// `path` это имена от корня через `/`, у корня просто его имя.
use std::cell::RefCell;
use std::rc::{Rc, Weak};

pub struct Node {
    name: String,
    parent: RefCell<Weak<Node>>,
    children: RefCell<Vec<Rc<Node>>>,
}

impl Node {
    pub fn new(name: &str) -> Rc<Node> {
        todo!()
    }

    pub fn add_child(parent: &Rc<Node>, child: Rc<Node>) {
        todo!("записать родителя ребёнку (слабо) и ребёнка родителю (сильно)")
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn parent(&self) -> Option<Rc<Node>> {
        todo!()
    }

    pub fn path(&self) -> String {
        todo!()
    }

    pub fn depth(&self) -> usize {
        todo!()
    }

    pub fn child_names(&self) -> Vec<String> {
        todo!()
    }
}

fn main() {
    let root = Node::new("root");
    let docs = Node::new("docs");
    Node::add_child(&root, docs.clone());
    println!("{}", docs.path());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> (Rc<Node>, Rc<Node>, Rc<Node>) {
        let root = Node::new("root");
        let docs = Node::new("docs");
        let notes = Node::new("notes");
        Node::add_child(&root, Rc::clone(&docs));
        Node::add_child(&docs, Rc::clone(&notes));
        (root, docs, notes)
    }

    #[test]
    fn paths_and_depth() {
        let (root, docs, notes) = tree();
        assert_eq!(root.path(), "root");
        assert_eq!(docs.path(), "root/docs");
        assert_eq!(notes.path(), "root/docs/notes");
        assert_eq!((root.depth(), docs.depth(), notes.depth()), (0, 1, 2));
        assert_eq!(root.child_names(), vec!["docs"]);
        assert_eq!(notes.parent().map(|p| p.name().to_string()), Some("docs".to_string()));
        assert!(root.parent().is_none());
    }

    #[test]
    fn down_is_strong_and_up_is_weak() {
        let (root, docs, notes) = tree();
        assert_eq!(Rc::strong_count(&root), 1, "ребёнок не должен держать родителя живым");
        assert_eq!(Rc::weak_count(&root), 1);
        assert_eq!(Rc::strong_count(&docs), 2, "docs держат наша переменная и список детей root");
        assert_eq!(Rc::weak_count(&docs), 1);
        assert_eq!(Rc::strong_count(&notes), 2);
    }

    #[test]
    fn orphans_forget_a_dropped_parent() {
        let (root, docs, _notes) = tree();
        drop(root);
        assert!(docs.parent().is_none(), "корня больше нет, слабая ссылка не оживляет его");
        assert_eq!(docs.path(), "docs", "осиротевшая папка считает себя корнем");
        assert_eq!(docs.depth(), 0);
    }

    #[test]
    fn several_children_keep_their_order() {
        let root = Node::new("r");
        for n in ["a", "b", "c"] {
            Node::add_child(&root, Node::new(n));
        }
        assert_eq!(root.child_names(), vec!["a", "b", "c"]);
        assert_eq!(Rc::weak_count(&root), 3);
    }
}
