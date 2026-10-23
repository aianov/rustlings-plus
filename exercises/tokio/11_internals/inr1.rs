// Воркеры планировщика не должны простаивать, пока у соседа очередь. Нужна модель выбора
// следующей задачи: у каждого воркера своя очередь, есть общая, а простой лечится воровством.
// Правила. Воркер берёт задачу с головы своей очереди. Но каждый `global_every`-й вызов воркера
// (считая все его вызовы с единицы, даже пустые) сначала смотрит в общую очередь, иначе она
// голодала бы при вечно полных локальных. Нет своих и общих: ворует у воркера с самой длинной
// очередью (при равенстве у меньшего номера) ровно половину, с округлением вверх, с головы.
// Первая украденная задача сразу выполняется, остальные встают в конец очереди вора.
// TODO: Напиши `next_task`.
use std::collections::VecDeque;

struct Scheduler {
    locals: Vec<VecDeque<u32>>,
    global: VecDeque<u32>,
    calls: Vec<u32>,
    global_every: u32,
}

impl Scheduler {
    fn new(workers: usize, global_every: u32) -> Scheduler {
        Scheduler { locals: vec![VecDeque::new(); workers], global: VecDeque::new(), calls: vec![0; workers], global_every }
    }

    fn push_local(&mut self, worker: usize, task: u32) {
        self.locals[worker].push_back(task);
    }

    fn push_global(&mut self, task: u32) {
        self.global.push_back(task);
    }

    fn local(&self, worker: usize) -> Vec<u32> {
        self.locals[worker].iter().copied().collect()
    }

    fn next_task(&mut self, worker: usize) -> Option<u32> {
        let _ = worker;
        todo!()
    }
}

fn main() {
    let mut sched = Scheduler::new(2, 61);
    sched.push_local(1, 10);
    println!("{:?}", sched.next_task(0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_queue_goes_first_in_order() {
        let mut s = Scheduler::new(2, 100);
        for t in [1, 2, 3] {
            s.push_local(0, t);
        }
        s.push_global(50);
        let got: Vec<_> = (0..3).map(|_| s.next_task(0).unwrap()).collect();
        assert_eq!(got, vec![1, 2, 3], "пока есть свои задачи, воркер берёт их по порядку, общая очередь подождёт");
    }

    #[test]
    fn idle_worker_takes_from_global_before_stealing() {
        let mut s = Scheduler::new(2, 100);
        s.push_global(7);
        s.push_global(8);
        s.push_local(1, 1);
        assert_eq!(s.next_task(0), Some(7), "нет своих: сначала общая очередь, воровство только после неё");
        assert_eq!(s.local(1), vec![1], "из очереди соседа ничего не должно пропасть, пока в общей есть задачи");
    }

    #[test]
    fn steals_half_rounded_up_from_the_head() {
        let mut s = Scheduler::new(3, 100);
        for t in [1, 2, 3, 4, 5] {
            s.push_local(1, t);
        }
        s.push_local(2, 9);
        assert_eq!(s.next_task(0), Some(1), "первая украденная задача выполняется сразу");
        assert_eq!(s.local(0), vec![2, 3], "из пяти крадут три (половина вверх) с головы: одну в работу, две вору в очередь");
        assert_eq!(s.local(1), vec![4, 5], "у жертвы остаётся хвост очереди");
        assert_eq!(s.local(2), vec![9], "третий воркер не жертва: его очередь короче");
    }

    #[test]
    fn victim_is_the_longest_and_ties_go_to_the_lowest_index() {
        let mut s = Scheduler::new(3, 100);
        for t in [1, 2] {
            s.push_local(1, t);
        }
        for t in [3, 4] {
            s.push_local(2, t);
        }
        assert_eq!(s.next_task(0), Some(1), "очереди равны: жертва воркер с меньшим номером");
        assert_eq!(s.local(2), vec![3, 4], "у воркера 2 ничего не забирали");
    }

    #[test]
    fn a_single_task_can_be_stolen_and_nothing_means_none() {
        let mut s = Scheduler::new(2, 100);
        assert_eq!(s.next_task(0), None, "везде пусто: задачи нет");
        s.push_local(1, 5);
        assert_eq!(s.next_task(0), Some(5), "половина от одной задачи, округлённая вверх, это сама задача");
        assert_eq!(s.local(1), Vec::<u32>::new(), "у жертвы ничего не осталось");
        assert_eq!(s.next_task(1), None, "воровать у себя нельзя, а больше задач нет");
    }

    #[test]
    fn every_nth_call_looks_at_the_global_queue_first() {
        let mut s = Scheduler::new(1, 3);
        for t in 1..=6 {
            s.push_local(0, t);
        }
        s.push_global(100);
        let got: Vec<_> = (0..5).map(|_| s.next_task(0).unwrap()).collect();
        assert_eq!(got, vec![1, 2, 100, 3, 4], "каждый третий вызов начинает с общей очереди, иначе её задачи ждали бы вечно");
    }

    #[test]
    fn the_global_turn_with_an_empty_global_queue_falls_back_to_local() {
        let mut s = Scheduler::new(1, 2);
        for t in 1..=4 {
            s.push_local(0, t);
        }
        let got: Vec<_> = (0..4).map(|_| s.next_task(0).unwrap()).collect();
        assert_eq!(got, vec![1, 2, 3, 4], "очередь общая пуста: очередь воркера идёт как обычно");
    }
}
