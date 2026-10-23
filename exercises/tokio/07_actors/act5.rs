// Превью картинок делает не одна задача, а пул воркеров: работы много, пусть трудятся вместе. Задания
// лежат в общей очереди, и каждый воркер берёт следующее, когда освободился. Приёмник у `mpsc` один,
// а воркеров несколько, и это надо как-то решить.
// TODO: Допиши `run_pool`: запусти `workers` воркеров на общей очереди и верни пары (номер воркера,
// результат `shrink`) по всем заданиям, каждое ровно один раз. Когда очередь опустела, воркеры выходят.
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

fn shrink(side: u32) -> u32 {
    side / 2
}

async fn run_pool(workers: usize, jobs: Vec<u32>) -> Vec<(usize, u32)> {
    todo!()
}

#[tokio::main]
async fn main() {
    println!("{:?}", run_pool(2, vec![100, 200, 300]).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn every_job_is_done_exactly_once() {
        let jobs: Vec<u32> = (1..=100).map(|n| n * 10).collect();
        let done = run_pool(4, jobs.clone()).await;
        let mut results: Vec<u32> = done.iter().map(|(_, r)| *r).collect();
        results.sort();
        let mut expected: Vec<u32> = jobs.iter().map(|j| shrink(*j)).collect();
        expected.sort();
        assert_eq!(results, expected, "каждое задание должно быть выполнено ровно один раз: ни потерь, ни повторов");
        assert!(done.iter().all(|(w, _)| *w < 4), "номера воркеров должны быть от 0 до workers - 1");
    }

    #[tokio::test]
    async fn one_worker_and_empty_queue_work_too() {
        let single = run_pool(1, vec![10, 20, 30]).await;
        assert_eq!(single.len(), 3, "один воркер обязан разобрать всю очередь");
        assert!(single.iter().all(|(w, _)| *w == 0), "единственный воркер имеет номер 0");
        let none = tokio::time::timeout(std::time::Duration::from_secs(1), run_pool(3, Vec::new()))
            .await
            .expect("пустая очередь: воркеры должны сразу выйти, а не ждать заданий");
        assert!(none.is_empty(), "заданий не было, результатов тоже");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn works_on_a_real_thread_pool() {
        let jobs: Vec<u32> = (0..1000).collect();
        let done = run_pool(8, jobs).await;
        assert_eq!(done.len(), 1000, "тысяча заданий, тысяча результатов");
        let total: u64 = done.iter().map(|(_, r)| *r as u64).sum();
        let expected: u64 = (0..1000u32).map(|j| shrink(j) as u64).sum();
        assert_eq!(total, expected, "сумма результатов не сошлась: какие-то задания потерялись или удвоились");
    }
}
