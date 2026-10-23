// На каком потоке выполняется твой код? Это зависит от вида рантайма, и разница видна
// невооружённым глазом. Нужны два измерителя: сколько разных потоков занято задачами
// и стартует ли spawn-нутая задача сама, без block_on. Задачи в опыте нарочно блокируют
// поток на 50 мс через std::thread::sleep, чтобы остальные воркеры успели подхватить работу.
// TODO: Допиши обе функции, дальше тесты расскажут разницу между current_thread и multi_thread.
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::runtime::{Builder, Runtime};

fn one_thread() -> Runtime {
    Builder::new_current_thread().enable_all().build().unwrap()
}

fn four_workers() -> Runtime {
    Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap()
}

fn distinct_threads(rt: &Runtime, tasks: usize) -> usize {
    todo!()
}

fn runs_without_block_on(rt: &Runtime) -> bool {
    todo!()
}

fn main() {
    for (name, rt) in [("current_thread", one_thread()), ("multi_thread", four_workers())] {
        println!(
            "{name}: потоков {}, сам стартует: {}",
            distinct_threads(&rt, 8),
            runs_without_block_on(&rt)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_thread_uses_exactly_one_thread() {
        assert_eq!(
            distinct_threads(&one_thread(), 8),
            1,
            "на current_thread все задачи идут на потоке, который вызвал block_on"
        );
    }

    #[test]
    fn multi_thread_spreads_tasks() {
        assert!(
            distinct_threads(&four_workers(), 8) >= 2,
            "блокирующие задачи на multi_thread должны разъехаться по нескольким воркерам"
        );
    }

    #[test]
    fn current_thread_waits_for_block_on() {
        assert!(
            !runs_without_block_on(&one_thread()),
            "current_thread не крутит задачи сам: пока никто не вызвал block_on, spawn-нутая задача стоит"
        );
    }

    #[test]
    fn multi_thread_runs_by_itself() {
        assert!(
            runs_without_block_on(&four_workers()),
            "у multi_thread есть свои потоки, задача должна выполниться без block_on"
        );
    }
}
