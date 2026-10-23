pub fn multiplier(factor: i32) -> impl Fn(i32) -> i32 {
    move |x| x * factor
}

pub fn chooser(negate: bool) -> Box<dyn Fn(i32) -> i32> {
    if negate { Box::new(|x| -x) } else { Box::new(|x| x) }
}

pub fn counter_from(start: u32) -> impl FnMut() -> u32 {
    let mut next = start;
    move || {
        let current = next;
        next += 1;
        current
    }
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
