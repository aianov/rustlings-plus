// Уведомления бывают срочные (вход с нового устройства) и обычные (кто-то лайкнул). Срочные уходят первыми,
// но между собой одного приоритета они идут по очереди прихода: иначе у двух одинаково важных
// сообщений порядок случайный. Вторая очередь планировщик: задания с временем запуска, первым достаётся
// самое раннее.
// TODO: Допиши `NotifyQueue` и `Scheduler` на `BinaryHeap`. Он отдаёт наибольший элемент, а нужен то
// наибольший приоритет, то наименьшее время. Вспомни про `Reverse`.
use std::cmp::Reverse;
use std::collections::BinaryHeap;

#[derive(Default)]
pub struct NotifyQueue {
    seq: u64,
    heap: BinaryHeap<(u8, Reverse<u64>, String)>,
}

#[derive(Default)]
pub struct Scheduler {
    heap: BinaryHeap<Reverse<(u64, u64)>>,
}

impl NotifyQueue {
    pub fn push(&mut self, priority: u8, text: &str) {
        todo!("положить с приоритетом и номером прихода")
    }

    pub fn pop(&mut self) -> Option<String> {
        todo!("самое важное; при равенстве самое раннее")
    }
}

impl Scheduler {
    pub fn schedule(&mut self, at: u64, job: u64) {
        todo!("запомнить задание на момент at")
    }

    pub fn pop_due(&mut self, now: u64) -> Vec<u64> {
        todo!("все задания с временем не позже now, самые ранние первыми")
    }

    pub fn next_at(&self) -> Option<u64> {
        todo!("время ближайшего задания")
    }
}

fn main() {
    let mut q = NotifyQueue::default();
    q.push(1, "like");
    println!("{:?}", q.pop());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urgent_goes_first() {
        let mut q = NotifyQueue::default();
        q.push(1, "like");
        q.push(9, "new device");
        q.push(5, "mention");
        assert_eq!(q.pop().as_deref(), Some("new device"));
        assert_eq!(q.pop().as_deref(), Some("mention"));
        assert_eq!(q.pop().as_deref(), Some("like"));
        assert_eq!(q.pop(), None);
    }

    #[test]
    fn equal_priority_keeps_arrival_order() {
        let mut q = NotifyQueue::default();
        for text in ["zeta", "alpha", "mid", "beta"] {
            q.push(3, text);
        }
        let order: Vec<String> = std::iter::from_fn(|| q.pop()).collect();
        assert_eq!(order, vec!["zeta", "alpha", "mid", "beta"], "одинаковые приоритеты идут по очереди прихода, а не по алфавиту");
    }

    #[test]
    fn the_scheduler_hands_out_the_earliest_first() {
        let mut s = Scheduler::default();
        s.schedule(50, 1);
        s.schedule(10, 2);
        s.schedule(30, 3);
        assert_eq!(s.next_at(), Some(10));
        assert_eq!(s.pop_due(5), Vec::<u64>::new(), "ничего ещё не наступило");
        assert_eq!(s.pop_due(30), vec![2, 3], "наступили 10 и 30, самое раннее первым");
        assert_eq!(s.next_at(), Some(50));
        assert_eq!(s.pop_due(1000), vec![1]);
        assert_eq!(s.next_at(), None);
    }
}
