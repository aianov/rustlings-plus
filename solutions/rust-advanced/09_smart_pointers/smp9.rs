use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Default, PartialEq)]
pub struct Inbox {
    pub messages: Vec<String>,
}

pub type Shared = Rc<RefCell<Inbox>>;

pub fn new_inbox() -> Shared {
    Rc::new(RefCell::new(Inbox::default()))
}

pub struct Topic {
    name: String,
    subscribers: Vec<Shared>,
}

impl Topic {
    pub fn new(name: &str) -> Topic {
        Topic { name: name.to_string(), subscribers: Vec::new() }
    }

    pub fn subscribe(&mut self, inbox: &Shared) {
        self.subscribers.push(Rc::clone(inbox));
    }

    pub fn unsubscribe(&mut self, inbox: &Shared) -> bool {
        let before = self.subscribers.len();
        self.subscribers.retain(|s| !Rc::ptr_eq(s, inbox));
        self.subscribers.len() != before
    }

    pub fn publish(&self, text: &str) -> usize {
        for inbox in &self.subscribers {
            inbox.borrow_mut().messages.push(format!("{}: {text}", self.name));
        }
        self.subscribers.len()
    }

    pub fn subscribers(&self) -> usize {
        self.subscribers.len()
    }
}

fn main() {
    let inbox = new_inbox();
    let mut news = Topic::new("news");
    news.subscribe(&inbox);
    news.publish("hello");
    println!("{:?}", inbox.borrow().messages);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_inbox_listens_to_two_topics() {
        let inbox = new_inbox();
        let (mut news, mut sport) = (Topic::new("news"), Topic::new("sport"));
        news.subscribe(&inbox);
        sport.subscribe(&inbox);
        assert_eq!(news.publish("storm"), 1);
        sport.publish("goal");
        news.publish("calm");
        assert_eq!(inbox.borrow().messages, vec!["news: storm", "sport: goal", "news: calm"], "письма обеих тем лежат в одном ящике владельца");
    }

    #[test]
    fn subscribing_shares_instead_of_copying() {
        let inbox = new_inbox();
        assert_eq!(Rc::strong_count(&inbox), 1);
        let mut t = Topic::new("t");
        t.subscribe(&inbox);
        assert_eq!(Rc::strong_count(&inbox), 2, "подписка это ещё один счётчик на тот же ящик");
        assert_eq!(t.subscribers(), 1);
    }

    #[test]
    fn unsubscribe_finds_the_inbox_by_identity() {
        let (a, b) = (new_inbox(), new_inbox());
        let mut t = Topic::new("t");
        t.subscribe(&a);
        t.subscribe(&b);
        assert!(t.unsubscribe(&b), "подписан был");
        assert!(!t.unsubscribe(&b), "второй раз уже нет");
        assert_eq!(t.subscribers(), 1, "второй пустой ящик с тем же содержимым не должен был отписаться");
        t.publish("x");
        assert_eq!(a.borrow().messages, vec!["t: x"]);
        assert!(b.borrow().messages.is_empty());
        assert_eq!(Rc::strong_count(&b), 1, "после отписки тема счётчик отпустила");
    }

    #[test]
    fn dropping_the_topic_releases_every_subscription() {
        let inbox = new_inbox();
        {
            let mut t = Topic::new("t");
            t.subscribe(&inbox);
            t.subscribe(&inbox);
            assert_eq!(Rc::strong_count(&inbox), 3);
            assert_eq!(t.publish("twice"), 2, "дважды подписанный ящик получает письмо дважды");
        }
        assert_eq!(Rc::strong_count(&inbox), 1);
        assert_eq!(inbox.borrow().messages.len(), 2);
    }
}
