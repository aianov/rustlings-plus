
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;

fn main() {}

#[derive(Default)]
struct State {
    serving: bool,
    running: bool,
    in_flight: usize,
}

pub struct Instance {
    state: Mutex<State>,
    restarts: AtomicU32,
}

impl Instance {
    fn new() -> Instance {
        Instance { state: Mutex::new(State { serving: true, running: true, in_flight: 0 }), restarts: AtomicU32::new(0) }
    }

    /// Входит ли экземпляр в круг балансировщика.
    pub fn set_serving(&self, serving: bool) {
        self.state.lock().unwrap().serving = serving;
    }

    pub fn serving(&self) -> bool {
        self.state.lock().unwrap().serving
    }

    pub fn in_flight(&self) -> usize {
        self.state.lock().unwrap().in_flight
    }

    pub fn restarts(&self) -> u32 {
        self.restarts.load(Ordering::SeqCst)
    }

    /// Останавливает процесс. Запросы, которые он ещё вёл, обрываются.
    pub async fn stop(&self) {
        self.state.lock().unwrap().running = false;
    }

    /// Запускает новую версию: готова она через секунду.
    pub async fn start(&self) {
        tokio::time::sleep(Duration::from_secs(1)).await;
        self.state.lock().unwrap().running = true;
        self.restarts.fetch_add(1, Ordering::SeqCst);
    }
}

pub struct Pool {
    pub instances: Vec<Instance>,
    next: AtomicUsize,
}

impl Pool {
    pub fn new(count: usize) -> Pool {
        Pool { instances: (0..count).map(|_| Instance::new()).collect(), next: AtomicUsize::new(0) }
    }

    pub fn serving_count(&self) -> usize {
        self.instances.iter().filter(|i| i.serving()).count()
    }

    /// Запрос пользователя: идёт к очередному экземпляру из круга, обрабатывается 50 мс. Ошибка, если круг пуст или процесс остановили посреди запроса.
    pub async fn request(&self) -> Result<(), &'static str> {
        let n = self.instances.len();
        let start = self.next.fetch_add(1, Ordering::SeqCst);
        let inst = (0..n).map(|k| &self.instances[(start + k) % n]).find(|i| i.serving()).ok_or("круг пуст")?;
        inst.state.lock().unwrap().in_flight += 1;
        tokio::time::sleep(Duration::from_millis(50)).await;
        let mut st = inst.state.lock().unwrap();
        st.in_flight -= 1;
        if st.running { Ok(()) } else { Err("экземпляр остановили посреди запроса") }
    }
}

/// Перезапускает все экземпляры по одному.
pub async fn rolling_restart(pool: &Pool) {
    for inst in &pool.instances {
        inst.set_serving(false);
        let drained = async {
            while inst.in_flight() > 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        let _ = tokio::time::timeout(Duration::from_secs(5), drained).await;
        inst.stop().await;
        inst.start().await;
        inst.set_serving(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn users_do_not_notice_the_rollout() {
        let pool = Arc::new(Pool::new(3));
        let failed = Arc::new(AtomicU32::new(0));
        let ok = Arc::new(AtomicU32::new(0));
        let min_serving = Arc::new(AtomicUsize::new(3));
        let stop_load = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let load = {
            let (pool, failed, ok, stop) = (pool.clone(), failed.clone(), ok.clone(), stop_load.clone());
            tokio::spawn(async move {
                let mut tasks = Vec::new();
                while !stop.load(Ordering::SeqCst) {
                    let (pool, failed, ok) = (pool.clone(), failed.clone(), ok.clone());
                    tasks.push(tokio::spawn(async move {
                        match pool.request().await {
                            Ok(()) => ok.fetch_add(1, Ordering::SeqCst),
                            Err(_) => failed.fetch_add(1, Ordering::SeqCst),
                        };
                    }));
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                for t in tasks {
                    let _ = t.await;
                }
            })
        };
        let watcher = {
            let (pool, min, stop) = (pool.clone(), min_serving.clone(), stop_load.clone());
            tokio::spawn(async move {
                while !stop.load(Ordering::SeqCst) {
                    min.fetch_min(pool.serving_count(), Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
        };

        tokio::time::sleep(Duration::from_millis(500)).await;
        let started = Instant::now();
        rolling_restart(&pool).await;
        let took = started.elapsed();
        tokio::time::sleep(Duration::from_millis(500)).await;
        stop_load.store(true, Ordering::SeqCst);
        load.await.unwrap();
        watcher.await.unwrap();

        assert_eq!(pool.instances.iter().map(Instance::restarts).collect::<Vec<_>>(), vec![1, 1, 1], "каждый экземпляр должен быть перезапущен ровно один раз");
        assert_eq!(failed.load(Ordering::SeqCst), 0, "во время выката пользователи получили {} ошибок: экземпляр остановили раньше, чем вывели из круга или дождались начатых запросов", failed.load(Ordering::SeqCst));
        assert!(ok.load(Ordering::SeqCst) > 150, "запросов обслужено мало: {}", ok.load(Ordering::SeqCst));
        assert!(min_serving.load(Ordering::SeqCst) >= 2, "в круге оставалось всего {} экземпляров из трёх: выкат должен выводить по одному", min_serving.load(Ordering::SeqCst));
        assert!(pool.serving_count() == 3, "после выката не все вернулись в круг");
        assert!(took < Duration::from_secs(6), "выкат занял {took:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn an_instance_returns_to_the_rotation_only_when_it_is_ready() {
        let pool = Arc::new(Pool::new(2));
        let restart = {
            let pool = pool.clone();
            tokio::spawn(async move { rolling_restart(&pool).await })
        };
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(!pool.instances[0].serving(), "первый экземпляр запускается: в круге его быть не должно");
        assert!(pool.instances[1].serving(), "второй ещё не тронут");
        assert_eq!(pool.instances[1].restarts(), 0);
        restart.await.unwrap();
        assert!(pool.instances.iter().all(Instance::serving));
    }

    #[tokio::test(start_paused = true)]
    async fn a_hung_request_does_not_block_the_rollout_forever() {
        let pool = Arc::new(Pool::new(2));
        pool.instances[0].state.lock().unwrap().in_flight = 1;
        let started = Instant::now();
        tokio::time::timeout(Duration::from_secs(60), rolling_restart(&pool)).await.expect("выкат завис на зависшем запросе");
        assert!(started.elapsed() < Duration::from_secs(10), "ждали зависший запрос {:?}: предел ожидания слива должен быть конечным", started.elapsed());
        assert_eq!(pool.instances[0].restarts(), 1);
    }
}
