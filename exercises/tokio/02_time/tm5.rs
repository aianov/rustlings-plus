// Запрос проходит цепочку шагов, и на всё про всё есть общий бюджет времени. Если дать каждому
// шагу отдельный таймаут, цепочка растянется в несколько раз дольше бюджета. Но у некоторых
// шагов есть ещё и свой, более строгий лимит.
// TODO: Допиши `run_chain`. Шаги идут по очереди, у шага пара (сколько он длится, свой лимит в мс).
// Все уложились: Ok(число шагов). Шаг не успел за свой лимит или общий бюджет кончился: Err(сколько
// шагов завершились), и это случается ровно в тот момент, который наступил раньше.
use std::time::Duration;

async fn step(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

async fn run_chain(steps: &[(u64, u64)], budget_ms: u64) -> Result<usize, usize> {
    todo!()
}

#[tokio::main]
async fn main() {
    println!("{:?}", run_chain(&[(40, 80), (40, 80), (40, 80)], 100).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn fits_into_the_budget() {
        let started = Instant::now();
        assert_eq!(run_chain(&[(30, 50), (30, 50), (30, 50)], 200).await, Ok(3), "уложились");
        assert_eq!(started.elapsed(), Duration::from_millis(90), "три шага по 30 мс");
    }

    #[tokio::test(start_paused = true)]
    async fn budget_is_shared_between_steps() {
        let started = Instant::now();
        let result = run_chain(&[(40, 80), (40, 80), (40, 80)], 100).await;
        assert_eq!(result, Err(2), "два шага по 40 мс успели, третий нет: общий бюджет 100 мс");
        assert_eq!(
            started.elapsed(),
            Duration::from_millis(100),
            "цепочка должна оборваться ровно на 100 мс. Бюджет общий, а не свой у каждого шага"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn step_own_limit_is_stricter_than_budget() {
        let started = Instant::now();
        let result = run_chain(&[(20, 50), (90, 30), (10, 50)], 500).await;
        assert_eq!(result, Err(1), "второй шаг длится 90 мс при своём лимите 30 мс: цепочка обрывается на нём");
        assert_eq!(
            started.elapsed(),
            Duration::from_millis(50),
            "первый шаг 20 мс плюс свой лимит второго 30 мс, а не весь общий бюджет"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn budget_wins_over_roomy_step_limit() {
        let started = Instant::now();
        let result = run_chain(&[(60, 1000), (60, 1000)], 100).await;
        assert_eq!(result, Err(1), "лимит шага щедрый, но общий бюджет 100 мс кончается раньше");
        assert_eq!(started.elapsed(), Duration::from_millis(100), "обрыв ровно в момент конца бюджета");
    }

    #[tokio::test(start_paused = true)]
    async fn first_step_alone_may_not_fit() {
        assert_eq!(run_chain(&[(300, 400), (10, 400)], 100).await, Err(0), "первый шаг не успел");
    }

    #[tokio::test(start_paused = true)]
    async fn empty_chain() {
        assert_eq!(run_chain(&[], 100).await, Ok(0), "нет шагов: нечего делать");
    }
}
