// Пакетная обработка: `Batch` копит задания (`Job`, их нельзя ни клонировать, ни создать по умолчанию) и
// метку. Через `&mut self` нужно уметь «забрать» поле, не имея права его переместить: из `&mut` значение
// просто так не вытащить («cannot move out of borrowed content»), но можно подменить чем-то на его место.
// Для этого есть `mem::take` (подставить значение по умолчанию), `mem::replace` (подставить нужное)
// и `mem::swap` (обменять два места).
// TODO: Реализуй `flush` (отдать все задания, оставив список пустым), `relabel` (поставить новую метку,
// вернув прежнюю) и `swap_queues` (поменять обычную и срочную очередь местами).

#[derive(Debug, PartialEq)]
pub struct Job {
    pub id: u32,
}

pub struct Batch {
    jobs: Vec<Job>,
    urgent: Vec<Job>,
    label: String,
}

impl Batch {
    pub fn new(label: &str) -> Batch {
        Batch { jobs: Vec::new(), urgent: Vec::new(), label: label.to_string() }
    }

    pub fn add(&mut self, id: u32) {
        self.jobs.push(Job { id });
    }

    pub fn add_urgent(&mut self, id: u32) {
        self.urgent.push(Job { id });
    }

    pub fn ids(&self) -> Vec<u32> {
        self.jobs.iter().map(|j| j.id).collect()
    }

    pub fn urgent_ids(&self) -> Vec<u32> {
        self.urgent.iter().map(|j| j.id).collect()
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn flush(&mut self) -> Vec<Job> {
        todo!()
    }

    pub fn relabel(&mut self, new_label: &str) -> String {
        todo!()
    }

    pub fn swap_queues(&mut self) {
        todo!()
    }
}

fn main() {
    let mut b = Batch::new("night");
    b.add(1);
    println!("{:?}", b.flush());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flush_hands_over_everything_and_leaves_an_empty_queue() {
        let mut b = Batch::new("night");
        b.add(1);
        b.add(2);
        let got = b.flush();
        assert_eq!(got, vec![Job { id: 1 }, Job { id: 2 }], "задания отдаются в исходном порядке и без копирования");
        assert!(b.ids().is_empty(), "после flush очередь пуста");
        b.add(3);
        assert_eq!(b.ids(), vec![3], "и ею можно продолжать пользоваться");
        assert!(Batch::new("x").flush().is_empty());
    }

    #[test]
    fn flush_leaves_the_urgent_queue_alone() {
        let mut b = Batch::new("n");
        b.add(1);
        b.add_urgent(9);
        b.flush();
        assert_eq!(b.urgent_ids(), vec![9]);
    }

    #[test]
    fn relabel_returns_the_old_label() {
        let mut b = Batch::new("night");
        assert_eq!(b.relabel("day"), "night");
        assert_eq!(b.label(), "day");
        assert_eq!(b.relabel("day"), "day");
    }

    #[test]
    fn swap_exchanges_the_queues() {
        let mut b = Batch::new("n");
        b.add(1);
        b.add(2);
        b.add_urgent(7);
        b.swap_queues();
        assert_eq!((b.ids(), b.urgent_ids()), (vec![7], vec![1, 2]));
        b.swap_queues();
        assert_eq!((b.ids(), b.urgent_ids()), (vec![1, 2], vec![7]));
    }
}
