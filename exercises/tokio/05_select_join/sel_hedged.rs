// Запрос к реплицированному хранилищу с хеджированием. Реплики быстрые не всегда: у одной сборка
// мусора, у другой сеть моргнула. Поэтому запрос сначала идёт к реплике 0, а если она молчит дольше
// `hedge_after`, к ней добавляется реплика 1 (и так далее, по одной). Вот правила:
//   1. Первый успешный ответ побеждает и возвращается сразу, все остальные запросы бросаются.
//   2. Ошибка реплики не прерывает работу: если ещё есть нетронутые реплики, следующая запускается
//      сразу, не дожидаясь `hedge_after`. Ошибки копятся.
//   3. Когда запущены все реплики, новых запусков нет, но ждём уже запущенные.
//   4. Все реплики ответили ошибкой: `AllFailed` с ошибками в порядке прихода.
//   5. Общий срок `deadline` считается от начала. Не успели: `Timeout`, все запросы бросаются.
// Реплик не меньше одной. Запуск реплики с номером i это вызов `call(i)`.
// TODO: Допиши `hedged`.
use std::future::Future;
use std::time::Duration;

#[derive(Debug, PartialEq)]
enum HedgeError<E> {
    Timeout,
    AllFailed(Vec<E>),
}

async fn hedged<T, E, F, Fut>(replicas: usize, hedge_after: Duration, deadline: Duration, call: F) -> Result<T, HedgeError<E>>
where
    F: Fn(usize) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    todo!()
}

#[tokio::main]
async fn main() {
    let result = hedged(3, Duration::from_millis(50), Duration::from_secs(1), |replica| async move {
        tokio::time::sleep(Duration::from_millis(if replica == 0 { 500 } else { 10 })).await;
        Ok::<_, String>(replica)
    })
    .await;
    println!("{result:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use futures::future::LocalBoxFuture;
    use std::cell::RefCell;
    use std::rc::Rc;
    use tokio::time::{Instant, sleep};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    type Reply = Result<u32, &'static str>;

    #[derive(Default)]
    struct Log {
        started: RefCell<Vec<(usize, Duration)>>,
        cancelled: RefCell<Vec<usize>>,
    }

    struct Guard {
        replica: usize,
        log: Rc<Log>,
        done: bool,
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            if !self.done {
                self.log.cancelled.borrow_mut().push(self.replica);
            }
        }
    }

    fn scripted(script: Vec<(u64, Reply)>, t0: Instant, log: Rc<Log>) -> impl Fn(usize) -> LocalBoxFuture<'static, Reply> {
        move |replica| {
            let (delay, reply) = script[replica];
            let log = Rc::clone(&log);
            log.started.borrow_mut().push((replica, t0.elapsed()));
            async move {
                let mut guard = Guard { replica, log, done: false };
                sleep(ms(delay)).await;
                guard.done = true;
                reply
            }
            .boxed_local()
        }
    }

    async fn run(script: Vec<(u64, Reply)>, hedge: u64, deadline: u64) -> (Result<u32, HedgeError<&'static str>>, Duration, Rc<Log>) {
        let log = Rc::new(Log::default());
        let t0 = Instant::now();
        let replicas = script.len();
        let result = hedged(replicas, ms(hedge), ms(deadline), scripted(script, t0, Rc::clone(&log))).await;
        let elapsed = t0.elapsed();
        (result, elapsed, log)
    }

    fn started(log: &Log) -> Vec<(usize, Duration)> {
        log.started.borrow().clone()
    }

    fn cancelled(log: &Log) -> Vec<usize> {
        let mut list = log.cancelled.borrow().clone();
        list.sort();
        list
    }

    #[tokio::test(start_paused = true)]
    async fn fast_first_replica_needs_no_help() {
        let (result, elapsed, log) = run(vec![(50, Ok(1)), (50, Ok(2))], 100, 1000).await;
        assert_eq!(result, Ok(1), "реплика 0 ответила вовремя");
        assert_eq!(elapsed, ms(50), "ответ вернулся сразу, как пришёл");
        assert_eq!(started(&log), vec![(0, ms(0))], "хеджировать не пришлось: вторая реплика не должна запускаться");
    }

    #[tokio::test(start_paused = true)]
    async fn silent_replica_gets_a_helper() {
        let (result, elapsed, log) = run(vec![(500, Ok(1)), (30, Ok(2))], 100, 1000).await;
        assert_eq!(result, Ok(2), "победила вторая реплика");
        assert_eq!(elapsed, ms(130), "вторая стартует на 100 мс и отвечает за 30 мс");
        assert_eq!(started(&log), vec![(0, ms(0)), (1, ms(100))], "вторая реплика стартует через hedge_after после первой");
        assert_eq!(cancelled(&log), vec![0], "медленная реплика 0 должна быть брошена, а не дожидаться");
    }

    #[tokio::test(start_paused = true)]
    async fn an_error_starts_the_next_one_at_once() {
        let (result, elapsed, log) = run(vec![(20, Err("boom")), (30, Ok(2))], 100, 1000).await;
        assert_eq!(result, Ok(2), "ошибка первой реплики не конец, вторая ответила");
        assert_eq!(elapsed, ms(50), "после ошибки на 20 мс вторая запускается сразу, а не через 100 мс");
        assert_eq!(started(&log), vec![(0, ms(0)), (1, ms(20))], "реплика 1 стартует в момент ошибки реплики 0");
    }

    #[tokio::test(start_paused = true)]
    async fn all_failed_reports_every_error_in_order() {
        let (result, elapsed, _) = run(vec![(10, Err("a")), (10, Err("b")), (10, Err("c"))], 100, 1000).await;
        assert_eq!(result, Err(HedgeError::AllFailed(vec!["a", "b", "c"])), "все три ошибки, в порядке прихода");
        assert_eq!(elapsed, ms(30), "реплики запускаются друг за другом по ошибке: 10, 20, 30 мс");
    }

    #[tokio::test(start_paused = true)]
    async fn deadline_cuts_everything() {
        let (result, elapsed, log) = run(vec![(10_000, Ok(1)), (10_000, Ok(2))], 100, 300).await;
        assert_eq!(result, Err(HedgeError::Timeout), "ни одна реплика не успела");
        assert_eq!(elapsed, ms(300), "общий срок считается от начала, а не от последнего запуска");
        assert_eq!(started(&log), vec![(0, ms(0)), (1, ms(100))], "запуски идут по расписанию");
        assert_eq!(cancelled(&log), vec![0, 1], "по истечении срока оба запроса бросаются");
    }

    #[tokio::test(start_paused = true)]
    async fn a_slow_winner_survives_other_errors() {
        let (result, elapsed, log) = run(vec![(250, Ok(1)), (20, Err("b")), (500, Ok(3))], 100, 1000).await;
        assert_eq!(result, Ok(1), "реплика 0 ответила на 250 мс, ошибки соседей её не отменяют");
        assert_eq!(elapsed, ms(250), "ждём реплику 0, а не прекращаем работу после ошибки реплики 1");
        assert_eq!(started(&log), vec![(0, ms(0)), (1, ms(100)), (2, ms(120))], "реплика 2 стартует сразу после ошибки реплики 1 (на 120 мс)");
        assert_eq!(cancelled(&log), vec![2], "брошена должна быть только реплика 2");
    }

    #[tokio::test(start_paused = true)]
    async fn single_replica_is_just_a_call() {
        let (result, elapsed, log) = run(vec![(500, Ok(7))], 100, 1000).await;
        assert_eq!(result, Ok(7), "единственную реплику надо дождаться, хеджировать некем");
        assert_eq!(elapsed, ms(500), "ждём ответ целиком");
        assert_eq!(started(&log), vec![(0, ms(0))], "других реплик нет");
        let (result, _, _) = run(vec![(10, Err("only"))], 100, 1000).await;
        assert_eq!(result, Err(HedgeError::AllFailed(vec!["only"])), "единственная реплика ошиблась: AllFailed с одной ошибкой");
    }
}
