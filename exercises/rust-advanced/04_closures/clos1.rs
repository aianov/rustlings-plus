// Конвейер обработки чисел собирается из шагов: удвоить, сдвинуть на смещение, возвести в квадрат.
// Шаги лежат в одном списке, потому что их порядок и состав потом будут настраиваться. Компилятор
// отказывается собирать `steps` и сообщает, что типы замыканий не совпадают, хотя все три выглядят
// совершенно одинаково: берут число и отдают число.
// TODO: Заставь `steps` собраться, не меняя поведения шагов, и допиши `run`.

pub fn steps(offset: i32) -> Vec<Box<dyn Fn(i32) -> i32>> {
    let double = |x| x * 2;
    let shift = move |x| x + offset;
    let square = |x| x * x;
    vec![double, shift, square]
}

pub fn run(steps: &[Box<dyn Fn(i32) -> i32>], input: i32) -> i32 {
    todo!("прогнать число через все шаги по порядку")
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
