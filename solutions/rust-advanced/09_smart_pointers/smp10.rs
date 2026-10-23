use std::cell::RefCell;
use std::rc::{Rc, Weak};

pub struct Node {
    name: String,
    parent: RefCell<Weak<Node>>,
    children: RefCell<Vec<Rc<Node>>>,
}

impl Node {
    pub fn new(name: &str) -> Rc<Node> {
        Rc::new(Node { name: name.to_string(), parent: RefCell::new(Weak::new()), children: RefCell::new(Vec::new()) })
    }

    pub fn add_child(parent: &Rc<Node>, child: Rc<Node>) {
        *child.parent.borrow_mut() = Rc::downgrade(parent);
        parent.children.borrow_mut().push(child);
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn parent(&self) -> Option<Rc<Node>> {
        self.parent.borrow().upgrade()
    }

    pub fn path(&self) -> String {
        match self.parent() {
            Some(parent) => format!("{}/{}", parent.path(), self.name),
            None => self.name.clone(),
        }
    }

    pub fn depth(&self) -> usize {
        self.parent().map_or(0, |p| p.depth() + 1)
    }

    pub fn child_names(&self) -> Vec<String> {
        self.children.borrow().iter().map(|c| c.name.clone()).collect()
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
