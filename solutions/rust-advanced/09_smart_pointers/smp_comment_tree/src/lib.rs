use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicUsize, Ordering};

static DICTIONARY_BUILDS: AtomicUsize = AtomicUsize::new(0);

pub fn dictionary_builds() -> usize {
    DICTIONARY_BUILDS.load(Ordering::SeqCst)
}

pub struct Live(Rc<Cell<usize>>);

pub struct Comment {
    author: String,
    text: RefCell<String>,
    parent: RefCell<Weak<Comment>>,
    children: RefCell<Vec<Rc<Comment>>>,
    live: Live,
}

pub struct Thread {
    roots: RefCell<Vec<Rc<Comment>>>,
    live: Rc<Cell<usize>>,
}

impl Live {
    fn new(counter: &Rc<Cell<usize>>) -> Live {
        counter.set(counter.get() + 1);
        Live(Rc::clone(counter))
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

type Tags = HashMap<&'static str, &'static str>;

static TAGS: LazyLock<Tags> = LazyLock::new(|| {
    DICTIONARY_BUILDS.fetch_add(1, Ordering::SeqCst);
    HashMap::from([("rust", "programming"), ("cats", "pets"), ("food", "life")])
});

fn detach(list: &RefCell<Vec<Rc<Comment>>>, item: &Rc<Comment>) -> bool {
    let mut list = list.borrow_mut();
    let before = list.len();
    list.retain(|c| !Rc::ptr_eq(c, item));
    list.len() != before
}

fn make(author: &str, text: &str, counter: &Rc<Cell<usize>>) -> Rc<Comment> {
    Rc::new(Comment {
        author: author.to_string(),
        text: RefCell::new(text.to_string()),
        parent: RefCell::new(Weak::new()),
        children: RefCell::new(Vec::new()),
        live: Live::new(counter),
    })
}

impl Thread {
    pub fn new() -> Thread {
        Thread { roots: RefCell::new(Vec::new()), live: Rc::new(Cell::new(0)) }
    }

    pub fn post(&self, author: &str, text: &str) -> Rc<Comment> {
        let comment = make(author, text, &self.live);
        self.roots.borrow_mut().push(Rc::clone(&comment));
        comment
    }

    pub fn remove(&self, comment: &Rc<Comment>) -> bool {
        let removed = match comment.parent() {
            Some(parent) => detach(&parent.children, comment),
            None => detach(&self.roots, comment),
        };
        if removed {
            *comment.parent.borrow_mut() = Weak::new();
        }
        removed
    }

    pub fn roots(&self) -> Vec<Rc<Comment>> {
        self.roots.borrow().clone()
    }

    pub fn total(&self) -> usize {
        self.roots.borrow().iter().map(|c| c.size()).sum()
    }

    pub fn live(&self) -> usize {
        self.live.get()
    }

    pub fn live_counter(&self) -> Rc<Cell<usize>> {
        Rc::clone(&self.live)
    }
}

impl Default for Thread {
    fn default() -> Thread {
        Thread::new()
    }
}

impl Comment {
    pub fn reply(self: &Rc<Comment>, author: &str, text: &str) -> Rc<Comment> {
        let child = make(author, text, &self.live.0);
        *child.parent.borrow_mut() = Rc::downgrade(self);
        self.children.borrow_mut().push(Rc::clone(&child));
        child
    }

    pub fn author(&self) -> &str {
        &self.author
    }

    pub fn text(&self) -> String {
        self.text.borrow().clone()
    }

    pub fn edit(&self, text: &str) {
        *self.text.borrow_mut() = text.to_string();
    }

    pub fn parent(&self) -> Option<Rc<Comment>> {
        self.parent.borrow().upgrade()
    }

    pub fn children(&self) -> Vec<Rc<Comment>> {
        self.children.borrow().clone()
    }

    pub fn path(&self) -> String {
        match self.parent() {
            Some(parent) => format!("{}/{}", parent.path(), self.author),
            None => self.author.clone(),
        }
    }

    pub fn depth(&self) -> usize {
        self.parent().map_or(0, |p| p.depth() + 1)
    }

    pub fn size(&self) -> usize {
        1 + self.children.borrow().iter().map(|c| c.size()).sum::<usize>()
    }

    pub fn tags(&self) -> Vec<&'static str> {
        let text = self.text.borrow();
        let mut found: Vec<&'static str> = Vec::new();
        for (at, _) in text.match_indices('#') {
            let name: String = text[at + 1..].chars().take_while(|c| c.is_alphanumeric()).collect();
            if let Some(tag) = TAGS.get(name.to_lowercase().as_str())
                && !found.contains(tag)
            {
                found.push(tag);
            }
        }
        found
    }
}
