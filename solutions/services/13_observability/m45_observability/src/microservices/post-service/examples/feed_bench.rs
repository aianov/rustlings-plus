use std::error::Error;
use std::io::Write;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use contracts::{Cause, Origin, UserId};
use post_service::repository::PostRepository;
use post_service::{Draft, MemoryPosts, PostText};
use tokio::runtime::Builder;

const USERS: u64 = 1_000;
const FOLLOWS: u64 = 200;
const POSTS: u64 = 50_000;
const PAGE: usize = 20;
const WARMUP: usize = 30;
const RUNS: usize = 300;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "ошибка: {error}");
            ExitCode::FAILURE
        }
    }
}

fn micros(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000_000.0
}

fn run() -> Result<(), Box<dyn Error>> {
    let runtime = Builder::new_current_thread().enable_all().build()?;
    let posts = MemoryPosts::new();
    runtime.block_on(async {
        for n in 0..POSTS {
            let author = UserId(n * 7_919 % USERS + 1);
            let draft = Draft {
                text: PostText::try_from(format!("пост {n}").as_str())?,
                tags: Vec::new(),
                links: Vec::new(),
            };
            let cause = Cause::new(&Origin::new("bench"), n);
            posts
                .insert(author, format!("user{}", author.0), draft, &cause)
                .await?;
        }
        Ok::<(), Box<dyn Error>>(())
    })?;
    let following: Vec<UserId> = (2..=USERS)
        .step_by((USERS / FOLLOWS) as usize)
        .map(UserId)
        .collect();
    let mut spent = Vec::with_capacity(RUNS);
    for round in 0..WARMUP + RUNS {
        let started = Instant::now();
        let page = runtime.block_on(posts.by_authors(following.clone(), 0, PAGE));
        if round >= WARMUP {
            spent.push(started.elapsed());
        }
        if page.len() != PAGE {
            return Err(format!("на странице {} постов вместо {PAGE}", page.len()).into());
        }
    }
    spent.sort_unstable();
    let median = spent[spent.len() / 2];
    let p99 = spent[spent.len() * 99 / 100];
    let mut out = std::io::stdout().lock();
    writeln!(
        out,
        "лента: {PAGE} постов на странице, {POSTS} постов в сети, {FOLLOWS} подписок"
    )?;
    writeln!(
        out,
        "медиана {:.1} мкс, p99 {:.1} мкс",
        micros(median),
        micros(p99)
    )?;
    Ok(())
}
