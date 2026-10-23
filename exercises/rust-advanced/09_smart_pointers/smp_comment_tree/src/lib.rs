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
        todo!("увеличить счётчик живых комментариев")
    }
}

impl Thread {
    pub fn new() -> Thread {
        Thread { roots: RefCell::new(Vec::new()), live: Rc::new(Cell::new(0)) }
    }

    pub fn post(&self, author: &str, text: &str) -> Rc<Comment> {
        todo!("корневой комментарий")
    }

    pub fn remove(&self, comment: &Rc<Comment>) -> bool {
        todo!("отцепить комментарий вместе с ветвью")
    }

    pub fn roots(&self) -> Vec<Rc<Comment>> {
        todo!()
    }

    pub fn total(&self) -> usize {
        todo!("сколько комментариев достижимо из корней")
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
        todo!()
    }

    pub fn author(&self) -> &str {
        &self.author
    }

    pub fn text(&self) -> String {
        todo!()
    }

    pub fn edit(&self, text: &str) {
        todo!()
    }

    pub fn parent(&self) -> Option<Rc<Comment>> {
        todo!()
    }

    pub fn children(&self) -> Vec<Rc<Comment>> {
        todo!()
    }

    pub fn path(&self) -> String {
        todo!("авторы от корня через /")
    }

    pub fn depth(&self) -> usize {
        todo!()
    }

    pub fn size(&self) -> usize {
        todo!("число комментариев в ветви, включая этот")
    }

    pub fn tags(&self) -> Vec<&'static str> {
        todo!("известные теги из текста, без повторов, в порядке появления")
    }
}
