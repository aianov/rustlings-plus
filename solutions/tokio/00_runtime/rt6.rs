use tokio::runtime::{Builder, Runtime};

enum Flavor {
    OneThread,
    Workers(usize),
}

fn build(flavor: Flavor) -> Runtime {
    let mut builder = match flavor {
        Flavor::OneThread => Builder::new_current_thread(),
        Flavor::Workers(count) => {
            let mut builder = Builder::new_multi_thread();
            builder.worker_threads(count);
            builder
        }
    };
    builder
        .thread_name("course-worker")
        .enable_all()
        .build()
        .expect("не удалось собрать рантайм")
}

fn main() {
    for flavor in [Flavor::OneThread, Flavor::Workers(2)] {
        let runtime = build(flavor);
        println!("воркеров: {}", runtime.handle().metrics().num_workers());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn one_thread_has_one_worker() {
        let runtime = build(Flavor::OneThread);
        assert_eq!(runtime.handle().metrics().num_workers(), 1, "current_thread: один воркер");
    }

    #[test]
    fn workers_count_is_respected() {
        let runtime = build(Flavor::Workers(3));
        assert_eq!(
            runtime.handle().metrics().num_workers(),
            3,
            "Workers(3) должен дать ровно три воркера: worker_threads"
        );
    }

    #[test]
    fn timers_and_sockets_are_enabled() {
        for flavor in [Flavor::OneThread, Flavor::Workers(2)] {
            let runtime = build(flavor);
            let port = runtime.block_on(async {
                tokio::time::sleep(Duration::from_millis(1)).await;
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                listener.local_addr().unwrap().port()
            });
            assert_ne!(port, 0, "и таймеры, и сокеты должны работать: нужен enable_all");
        }
    }

    #[test]
    fn workers_have_a_name() {
        let runtime = build(Flavor::Workers(2));
        let name = runtime.block_on(async {
            tokio::spawn(async { std::thread::current().name().map(str::to_string) })
                .await
                .unwrap()
        });
        assert_eq!(
            name.as_deref(),
            Some("course-worker"),
            "поток воркера должен называться course-worker: thread_name"
        );
    }
}
