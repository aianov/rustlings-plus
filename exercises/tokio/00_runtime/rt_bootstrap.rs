// Точка входа без макроса. Приложение запускается синхронным кодом: разобрать конфиг,
// собрать рантайм нужного вида, отдать ему async-ядро, получить отчёт. А ещё к уже
// работающему рантайму должен уметь обратиться чужой, обычный поток.
// Конфиг разбирается готовой parse_config. Тебе осталось:
// 1. build_runtime: workers = 0 это ошибка NoWorkers (Builder на ноль паникует!), 1 это
//    current_thread, больше это multi_thread с таким числом воркеров; потоки называются
//    "<name>-worker", включены все драйверы; ошибку сборки заверни в BootError::Runtime.
// 2. boot: запусти `tasks` задач, задача i спит 5 мс и возвращает i*i (i от 1); сложи результаты
//    в Report. Паника задачи (JoinError) уже превращается в BootError::Task через `?`.
// 3. run: разобрать, собрать, выполнить boot на собранном рантайме (синхронная граница).
// 4. report_from_thread: обычный поток, который выполняет boot на чужом рантайме через Handle.
use std::num::ParseIntError;
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::runtime::{Builder, Handle, Runtime};
use tokio::task::JoinError;

#[derive(Debug, PartialEq)]
pub enum BootError {
    BadConfig(String),
    NoWorkers,
    Runtime(String),
    Task(String),
}

impl From<ParseIntError> for BootError {
    fn from(error: ParseIntError) -> Self {
        BootError::BadConfig(error.to_string())
    }
}

impl From<JoinError> for BootError {
    fn from(error: JoinError) -> Self {
        BootError::Task(error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub name: String,
    pub workers: usize,
    pub tasks: u32,
}

#[derive(Debug, PartialEq)]
pub struct Report {
    pub name: String,
    pub tasks: u32,
    pub total: u32,
}

fn missing(key: &str) -> BootError {
    BootError::BadConfig(format!("нет ключа `{key}`"))
}

pub fn parse_config(text: &str) -> Result<Config, BootError> {
    let (mut name, mut workers, mut tasks) = (None, None, None);
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| BootError::BadConfig(format!("нет знака = в строке `{line}`")))?;
        match key.trim() {
            "name" => name = Some(value.trim().to_string()),
            "workers" => workers = Some(value.trim().parse()?),
            "tasks" => tasks = Some(value.trim().parse()?),
            other => return Err(BootError::BadConfig(format!("неизвестный ключ `{other}`"))),
        }
    }
    Ok(Config {
        name: name.ok_or_else(|| missing("name"))?,
        workers: workers.ok_or_else(|| missing("workers"))?,
        tasks: tasks.ok_or_else(|| missing("tasks"))?,
    })
}

pub fn build_runtime(config: &Config) -> Result<Runtime, BootError> {
    todo!()
}

pub async fn boot(config: Config) -> Result<Report, BootError> {
    todo!()
}

pub fn run(text: &str) -> Result<Report, BootError> {
    todo!()
}

pub fn report_from_thread(handle: &Handle, config: Config) -> JoinHandle<Result<Report, BootError>> {
    todo!()
}

fn main() {
    let text = "name=demo\nworkers=2\ntasks=4";
    match run(text) {
        Ok(report) => println!("{}: задач {}, сумма {}", report.name, report.tasks, report.total),
        Err(error) => eprintln!("не запустились: {error:?}"),
    }
    let config = parse_config(text).unwrap();
    let runtime = build_runtime(&config).unwrap();
    let report = report_from_thread(runtime.handle(), config).join().unwrap();
    println!("из чужого потока: {report:?}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(workers: usize, tasks: u32) -> Config {
        Config { name: "demo".to_string(), workers, tasks }
    }

    #[test]
    fn config_errors_are_reported() {
        assert!(matches!(parse_config("name=a\nworkers=x\ntasks=1"), Err(BootError::BadConfig(_))));
        assert!(matches!(parse_config("name=a\nworkers=1"), Err(BootError::BadConfig(_))));
        assert!(matches!(parse_config("name=a\nworkers=1\ntasks=1\nfoo=2"), Err(BootError::BadConfig(_))));
    }

    #[test]
    fn zero_workers_is_an_error_not_a_panic() {
        assert!(
            matches!(build_runtime(&config(0, 1)), Err(BootError::NoWorkers)),
            "workers = 0 должен дать BootError::NoWorkers: Builder::worker_threads(0) паникует"
        );
    }

    #[test]
    fn runtime_flavor_follows_workers() {
        let one = build_runtime(&config(1, 1)).unwrap();
        assert_eq!(one.handle().metrics().num_workers(), 1, "workers = 1: current_thread");
        let three = build_runtime(&config(3, 1)).unwrap();
        assert_eq!(three.handle().metrics().num_workers(), 3, "workers = 3: три воркера");
    }

    #[test]
    fn workers_are_named_after_the_app() {
        let runtime = build_runtime(&config(2, 1)).unwrap();
        let name = runtime.block_on(async {
            tokio::spawn(async { std::thread::current().name().map(str::to_string) })
                .await
                .unwrap()
        });
        assert_eq!(name.as_deref(), Some("demo-worker"), "потоки называются <name>-worker");
    }

    #[test]
    fn drivers_are_enabled() {
        let runtime = build_runtime(&config(1, 1)).unwrap();
        let port = runtime.block_on(async {
            tokio::time::sleep(Duration::from_millis(1)).await;
            tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap().port()
        });
        assert_ne!(port, 0, "нужны и таймеры, и сеть: enable_all");
    }

    #[test]
    fn sync_main_runs_the_async_core() {
        let report = run("name=demo\nworkers=2\ntasks=4\n").unwrap();
        assert_eq!(
            report,
            Report { name: "demo".to_string(), tasks: 4, total: 30 },
            "сумма квадратов 1..=4 равна 30"
        );
        let single = run("workers=1\nname=solo\ntasks=3").unwrap();
        assert_eq!(single.total, 14, "на current_thread то же самое");
    }

    #[test]
    fn run_passes_errors_up() {
        assert_eq!(run("name=a\nworkers=0\ntasks=1"), Err(BootError::NoWorkers));
        assert!(matches!(run("nonsense"), Err(BootError::BadConfig(_))));
    }

    #[tokio::test(start_paused = true)]
    async fn tasks_run_concurrently() {
        let started = tokio::time::Instant::now();
        let report = boot(config(1, 50)).await.unwrap();
        assert_eq!(report.total, 42925, "сумма квадратов 1..=50");
        assert!(
            started.elapsed() < Duration::from_millis(20),
            "50 задач по 5 мс заняли {:?}: задачи работали по очереди. Нужны именно параллельные задачи (spawn), тогда хватит около 5 мс",
            started.elapsed()
        );
    }

    #[test]
    fn plain_thread_can_use_a_running_runtime() {
        let runtime = build_runtime(&config(2, 4)).unwrap();
        let thread = report_from_thread(runtime.handle(), config(2, 4));
        let report = thread.join().expect("поток упал паникой").unwrap();
        assert_eq!(report.total, 30, "чужой поток получает тот же отчёт через Handle::block_on");
    }
}
