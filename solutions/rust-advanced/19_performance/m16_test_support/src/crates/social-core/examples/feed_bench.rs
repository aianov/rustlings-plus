use std::error::Error;
use std::process::ExitCode;

use social_core::{MemNetwork, SharedNetwork, UserId, Visibility, execute_line};
use test_support::{FakeClock, measure};

const USERS: usize = 1_000;
const FOLLOWS: usize = 200;
const POSTS: usize = 50_000;
const PAGE: usize = 20;
const WARMUP: usize = 30;
const RUNS: usize = 300;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ошибка: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let clock = FakeClock::new(1_700_000_000);
    let mut network = MemNetwork::new().with_clock(clock.clone());
    let users = (0..USERS)
        .map(|n| network.register(&format!("user{n}")))
        .collect::<Result<Vec<UserId>, _>>()?;
    let reader = *users.first().ok_or("в сети нет пользователей")?;
    for &author in users.iter().skip(1).step_by(USERS / FOLLOWS) {
        network.follow(reader, author)?;
    }
    for n in 0..POSTS {
        let author = users[n * 7_919 % USERS];
        clock.advance(1);
        network.publish(author, &format!("пост {n} #bench"), Visibility::Public)?;
    }
    let net = SharedNetwork::new(network)?;
    let line = format!("feed user0 {PAGE}");
    execute_line(&line, &net)?;
    let summary = measure(WARMUP, RUNS, || execute_line(&line, &net)).ok_or("замеров нет")?;
    println!("лента user0: {PAGE} постов на странице, {POSTS} постов в сети, {FOLLOWS} подписок");
    println!("{summary}");
    Ok(())
}
