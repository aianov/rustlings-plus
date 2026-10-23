// Макросы гигиеничны: имя, которое макрос создаёт у себя внутри, и имя вызывающего живут в разных мирах,
// даже если буквы одинаковые. Из этого два следствия. Во-первых, блок, который ты передаёшь в макрос,
// не видит переменных самого макроса. Во-вторых, макрос не видит локальных переменных вызывающего,
// пока их не передали. В заготовке нарушены оба правила: `squares` и `count_events` не собираются
// («cannot find value `idx` in this scope», «cannot find value `counter`»).
// Что нужно: `repeat!(имя, сколько, { блок })` повторяет блок, а переменная цикла называется так, как
// просит вызывающий; `bump!(имя)` увеличивает переменную вызывающего на 1; `swap_values!(a, b)`
// меняет два значения местами через временную переменную внутри макроса (даже если у вызывающего
// есть своя переменная `tmp`).
// TODO: Допиши макросы и поправь места их вызова в `squares` и `count_events`.
#![allow(unused_macros, clippy::manual_swap)]

macro_rules! repeat {
    ($times:expr, $body:block) => {
        for idx in 0..$times $body
    };
}

macro_rules! bump {
    () => {
        counter += 1;
    };
}

macro_rules! swap_values {
    () => {};
}

pub fn squares() -> Vec<u32> {
    let mut out = Vec::new();
    repeat!(3, {
        out.push(idx * idx);
    });
    out
}

pub fn count_events(events: &[&str]) -> u32 {
    let mut counter = 0;
    for _ in events {
        bump!();
    }
    counter
}

fn main() {
    println!("{:?} {}", squares(), count_events(&["a"]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_loop_variable_has_the_name_the_caller_chose() {
        let mut log = Vec::new();
        repeat!(step, 3, {
            log.push(step * 10);
        });
        assert_eq!(log, vec![0, 10, 20]);
    }

    #[test]
    fn the_block_still_sees_the_callers_own_variables() {
        let idx = 100;
        let mut total = 0;
        repeat!(n, 2, {
            total += idx + n;
        });
        assert_eq!(total, 201, "блок принадлежит вызывающему, свои переменные он видит");
    }

    #[test]
    fn the_existing_helpers_work_again() {
        assert_eq!(squares(), vec![0, 1, 4]);
        assert_eq!(count_events(&["a", "b", "c", "d"]), 4);
        assert_eq!(count_events(&[]), 0);
    }

    #[test]
    fn bump_changes_the_callers_variable() {
        let mut hits = 5;
        bump!(hits);
        bump!(hits);
        assert_eq!(hits, 7);
    }

    #[test]
    fn swap_values_uses_a_private_temporary() {
        let mut left = String::from("left");
        let mut right = String::from("right");
        swap_values!(left, right);
        assert_eq!((left.as_str(), right.as_str()), ("right", "left"));
    }

    #[test]
    fn swap_values_does_not_clash_with_a_caller_named_tmp() {
        let mut tmp = 1;
        let mut other = 2;
        swap_values!(tmp, other);
        assert_eq!((tmp, other), (2, 1), "tmp вызывающего и tmp макроса это разные переменные");
        let mut pair = (1, 2);
        swap_values!(pair.0, pair.1);
        assert_eq!(pair, (2, 1));
    }
}
