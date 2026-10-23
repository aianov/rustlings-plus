//! Пул потоков на std: очередь заданий, результаты через канал, ожидание на Condvar.
//!
//! TODO: Реализуй пул по README. Типы объявлены, поля добавь сам: тебе понадобятся общая очередь под
//! замком, счётчик выполняющихся заданий, флаг закрытия, два Condvar (для рабочих и для `wait_idle`)
//! и список потоков для `Drop`.

use std::marker::PhantomData;

#[derive(Debug, PartialEq)]
pub struct PoolClosed;

#[derive(Debug, PartialEq)]
pub enum JobError {
    Panicked,
}

pub struct JobHandle<T> {
    marker: PhantomData<T>,
}

impl<T> JobHandle<T> {
    pub fn join(self) -> Result<T, JobError> {
        todo!("дождаться результата; если задание упало, JobError::Panicked")
    }
}

pub struct ThreadPool {}

impl ThreadPool {
    pub fn new(workers: usize) -> ThreadPool {
        todo!("запустить рабочих")
    }

    pub fn workers(&self) -> usize {
        todo!("сколько рабочих")
    }

    pub fn execute(&self, job: impl FnOnce() + Send + 'static) -> Result<(), PoolClosed> {
        todo!("положить задание в очередь")
    }

    pub fn submit<T: Send + 'static>(&self, job: impl FnOnce() -> T + Send + 'static) -> Result<JobHandle<T>, PoolClosed> {
        todo!("то же, но результат придёт через JobHandle")
    }

    pub fn wait_idle(&self) {
        todo!("ждать, пока очередь пуста и ничего не выполняется")
    }

    pub fn pending(&self) -> usize {
        todo!("сколько заданий ждёт в очереди")
    }

    pub fn close(&self) {
        todo!("перестать принимать новые задания")
    }
}
