pub fn steps(offset: i32) -> Vec<Box<dyn Fn(i32) -> i32>> {
    let double = |x| x * 2;
    let shift = move |x| x + offset;
    let square = |x| x * x;
    vec![Box::new(double), Box::new(shift), Box::new(square)]
}

pub fn run(steps: &[Box<dyn Fn(i32) -> i32>], input: i32) -> i32 {
    steps.iter().fold(input, |acc, step| step(acc))
}

fn main() {
    println!("{}", run(&steps(3), 5));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_steps_in_order() {
        assert_eq!(steps(3).len(), 3, "в конвейере три шага");
        assert_eq!(run(&steps(3), 5), 169, "5 * 2 = 10, 10 + 3 = 13, 13 * 13 = 169");
    }

    #[test]
    fn offset_is_captured_per_pipeline() {
        let a = steps(1);
        let b = steps(100);
        assert_eq!(run(&a, 0), 1, "0 * 2 + 1, в квадрате");
        assert_eq!(run(&b, 0), 10_000, "смещение 100 запомнилось в своём конвейере");
        assert_eq!(run(&a, 0), 1, "первый конвейер не должен пострадать от второго");
    }

    #[test]
    fn custom_pipeline_and_empty_one() {
        let custom: Vec<Box<dyn Fn(i32) -> i32>> = vec![Box::new(|x| x - 1), Box::new(|x| x * 10)];
        assert_eq!(run(&custom, 4), 30);
        assert_eq!(run(&[], 7), 7, "пустой конвейер возвращает вход");
    }
}
