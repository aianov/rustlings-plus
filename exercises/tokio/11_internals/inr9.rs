// В health-эндпоинт сервиса надо вывести то, что рантайм знает о себе: сколько воркеров, сколько
// задач ещё живо и сколько стоит в очереди на запуск. А ещё нужен детектор утечки сценария:
// сколько задач осталось жить после него, когда все честные хвосты уже должны были закончиться.
// TODO: Напиши `snapshot` и `leaked`.
use std::future::Future;
use std::time::Duration;
use tokio::runtime::Handle;

#[derive(Debug, PartialEq)]
struct Snapshot {
    workers: usize,
    alive_tasks: usize,
    queued: usize,
}

fn snapshot(handle: &Handle) -> Snapshot {
    let _ = handle;
    todo!()
}

async fn leaked<F: Future>(scenario: F, grace: Duration) -> usize {
    let _ = (scenario, grace);
    todo!()
}

fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    println!("{:?}", snapshot(rt.handle()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::pending;
    use tokio::runtime::Builder;

    fn current_thread() -> tokio::runtime::Runtime {
        Builder::new_current_thread().enable_all().build().unwrap()
    }

    #[test]
    fn tasks_queued_from_outside_are_visible() {
        let rt = current_thread();
        for _ in 0..3 {
            rt.spawn(async {});
        }
        assert_eq!(
            snapshot(rt.handle()),
            Snapshot { workers: 1, alive_tasks: 3, queued: 3 },
            "три задачи поставлены снаружи рантайма: они живы и стоят в очереди запуска"
        );
        rt.block_on(tokio::task::yield_now());
        assert_eq!(
            snapshot(rt.handle()),
            Snapshot { workers: 1, alive_tasks: 0, queued: 0 },
            "задачи отработали и ушли: живых нет, очередь пуста"
        );
    }

    #[test]
    fn sleeping_tasks_are_alive_but_not_queued() {
        let rt = current_thread();
        for _ in 0..2 {
            rt.spawn(pending::<()>());
        }
        rt.block_on(tokio::task::yield_now());
        assert_eq!(
            snapshot(rt.handle()),
            Snapshot { workers: 1, alive_tasks: 2, queued: 0 },
            "две задачи ждут вечно: очередь пуста, но задачи живы"
        );
    }

    #[test]
    fn workers_follow_the_builder() {
        let rt = Builder::new_multi_thread().worker_threads(3).enable_all().build().unwrap();
        assert_eq!(snapshot(rt.handle()).workers, 3, "число воркеров должно браться из рантайма");
    }

    #[tokio::test(start_paused = true)]
    async fn finished_tails_are_not_leaks() {
        let lost = leaked(
            async {
                for ms in [10, 20, 30] {
                    tokio::spawn(tokio::time::sleep(Duration::from_millis(ms)));
                }
            },
            Duration::from_millis(100),
        )
        .await;
        assert_eq!(lost, 0, "три задачи закончились в пределах отсрочки: утечки нет");
    }

    #[tokio::test(start_paused = true)]
    async fn forgotten_tasks_are_counted() {
        let lost = leaked(
            async {
                tokio::spawn(pending::<()>());
                tokio::spawn(tokio::time::sleep(Duration::from_millis(10)));
                tokio::spawn(pending::<()>());
            },
            Duration::from_millis(100),
        )
        .await;
        assert_eq!(lost, 2, "две задачи не кончаются никогда, третья закончилась");
    }
}
