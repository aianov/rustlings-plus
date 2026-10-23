// Загрузка файла проходит ряд состояний: ждёт очереди, идёт, на паузе, готова, упала или отменена.
// События приходят откуда угодно (кнопки, сеть, таймеры), и часть из них не имеет смысла в текущем
// состоянии: нельзя поставить на паузу то, что уже закончилось. Автомат должен знать свою таблицу
// переходов наизусть и громко отказывать в остальном, возвращая и состояние, и событие, которые не
// сошлись, чтобы вызывающий мог записать их в журнал.
// Таблица допустимого:
//   Queued   + Start{total}  идёт (done 0); total 0 сразу Finished     Queued + Cancel   Cancelled
//   Running  + Progress(n)   done + n; дошли до total или дальше: Finished
//   Running  + Pause         Paused     Running + Fail(why) Failed(why)   Running + Cancel Cancelled
//   Paused   + Resume        Running    Paused  + Fail(why) Failed(why)   Paused  + Cancel Cancelled
// Всё остальное (в том числе любые события в Finished, Failed, Cancelled) это ошибка `Invalid`.
// TODO: Реализуй `next`. Переход берёт состояние и событие по значению.

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Queued,
    Running { done: u64, total: u64 },
    Paused { done: u64, total: u64 },
    Finished,
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Start { total: u64 },
    Progress(u64),
    Pause,
    Resume,
    Fail(String),
    Cancel,
}

#[derive(Debug, PartialEq)]
pub struct Invalid {
    pub state: State,
    pub event: Event,
}

pub fn next(state: State, event: Event) -> Result<State, Invalid> {
    todo!("таблица переходов")
}

fn main() {
    println!("{:?}", next(State::Queued, Event::Start { total: 10 }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn states() -> Vec<State> {
        vec![
            State::Queued,
            State::Running { done: 3, total: 10 },
            State::Paused { done: 3, total: 10 },
            State::Finished,
            State::Failed("disk full".into()),
            State::Cancelled,
        ]
    }

    fn events() -> Vec<Event> {
        vec![Event::Start { total: 10 }, Event::Progress(1), Event::Pause, Event::Resume, Event::Fail("boom".into()), Event::Cancel]
    }

    fn allowed(state: &State, event: &Event) -> bool {
        matches!(
            (state, event),
            (State::Queued, Event::Start { .. } | Event::Cancel)
                | (State::Running { .. }, Event::Progress(_) | Event::Pause | Event::Fail(_) | Event::Cancel)
                | (State::Paused { .. }, Event::Resume | Event::Fail(_) | Event::Cancel)
        )
    }

    #[test]
    fn exactly_the_table_is_allowed() {
        for s in states() {
            for e in events() {
                let result = next(s.clone(), e.clone());
                if allowed(&s, &e) {
                    assert!(result.is_ok(), "{e:?} в состоянии {s:?} допустимо, а получили {result:?}");
                } else {
                    assert_eq!(
                        result,
                        Err(Invalid { state: s.clone(), event: e.clone() }),
                        "{e:?} в состоянии {s:?} недопустимо: нужен отказ с теми же состоянием и событием"
                    );
                }
            }
        }
    }

    #[test]
    fn happy_path() {
        let s = next(State::Queued, Event::Start { total: 10 }).expect("старт");
        assert_eq!(s, State::Running { done: 0, total: 10 });
        let s = next(s, Event::Progress(4)).expect("прогресс");
        assert_eq!(s, State::Running { done: 4, total: 10 });
        let s = next(s, Event::Pause).expect("пауза");
        assert_eq!(s, State::Paused { done: 4, total: 10 }, "пауза помнит прогресс");
        let s = next(s, Event::Resume).expect("продолжение");
        assert_eq!(s, State::Running { done: 4, total: 10 }, "продолжение тоже");
        assert_eq!(next(s, Event::Progress(6)), Ok(State::Finished), "ровно до конца это Finished");
    }

    #[test]
    fn progress_edge_cases() {
        let run = || State::Running { done: 8, total: 10 };
        assert_eq!(next(run(), Event::Progress(1)), Ok(State::Running { done: 9, total: 10 }));
        assert_eq!(next(run(), Event::Progress(50)), Ok(State::Finished), "перелёт тоже конец");
        assert_eq!(next(run(), Event::Progress(u64::MAX)), Ok(State::Finished), "переполнения быть не должно");
        assert_eq!(next(run(), Event::Progress(0)), Ok(run()), "нулевой прогресс ничего не меняет, но допустим");
    }

    #[test]
    fn empty_download_finishes_at_once() {
        assert_eq!(next(State::Queued, Event::Start { total: 0 }), Ok(State::Finished));
    }

    #[test]
    fn failures_and_cancels_carry_over() {
        assert_eq!(next(State::Running { done: 1, total: 2 }, Event::Fail("x".into())), Ok(State::Failed("x".into())));
        assert_eq!(next(State::Paused { done: 1, total: 2 }, Event::Fail("y".into())), Ok(State::Failed("y".into())));
        for s in [State::Queued, State::Running { done: 1, total: 2 }, State::Paused { done: 1, total: 2 }] {
            assert_eq!(next(s.clone(), Event::Cancel), Ok(State::Cancelled), "отменить можно из {s:?}");
        }
    }
}
