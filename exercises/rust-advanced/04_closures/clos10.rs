// Три фабрики возвращают замыкания. `multiplier` запоминает множитель, `chooser` выбирает одно из двух
// действий, `counter_from` отдаёт счётчик с собственным состоянием. Компилятор отвергает первую фабрику
// (замыкание ссылается на параметр, который исчезнет вместе с вызовом) и вторую (две ветки дают
// два разных типа, а `impl Fn` это один конкретный тип). Третья собирается, но её ещё надо дописать.
// TODO: Почини `multiplier` и `chooser`, допиши `counter_from`: первый вызов даёт `start`,
// каждый следующий на единицу больше.

pub fn multiplier(factor: i32) -> impl Fn(i32) -> i32 {
    |x| x * factor
}

pub fn chooser(negate: bool) -> impl Fn(i32) -> i32 {
    if negate { |x| -x } else { |x| x }
}

pub fn counter_from(start: u32) -> impl FnMut() -> u32 {
    todo!("состояние живёт внутри замыкания")
}

fn main() {
    println!("{}", multiplier(3)(4));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiplier_remembers_its_factor() {
        let by_three = multiplier(3);
        let by_ten = multiplier(10);
        assert_eq!(by_three(4), 12);
        assert_eq!(by_ten(4), 40);
        assert_eq!(by_three(5), 15, "множители не путаются между собой");
    }

    #[test]
    fn chooser_picks_branch() {
        assert_eq!(chooser(true)(5), -5);
        assert_eq!(chooser(false)(5), 5);
        let both = [chooser(true), chooser(false)];
        assert_eq!(both.iter().map(|f| f(2)).collect::<Vec<_>>(), vec![-2, 2], "оба результата одного типа и лежат в одном массиве");
    }

    #[test]
    fn counters_have_private_state() {
        let mut a = counter_from(10);
        let mut b = counter_from(100);
        assert_eq!((a(), a(), a()), (10, 11, 12));
        assert_eq!(b(), 100, "второй счётчик начинает со своего значения");
        assert_eq!(a(), 13, "и не влияет на первый");
    }
}
