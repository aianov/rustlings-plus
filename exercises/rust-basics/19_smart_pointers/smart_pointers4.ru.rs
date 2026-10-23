// Это упражнение про умный указатель `Cow` (Clone-On-Write, «клонировать при
// записи»). Он может оборачивать заимствованные данные и давать неизменяемый
// доступ к ним, лениво клонируя данные, когда нужны изменение или владение.
// Тип рассчитан на работу с произвольными заимствованными данными через трейт `Borrow`.

use std::borrow::Cow;

fn abs_all(input: &mut Cow<[i32]>) {
    for ind in 0..input.len() {
        let value = input[ind];
        if value < 0 {
            // Клонирует в вектор, если данные ещё не принадлежат нам.
            input.to_mut()[ind] = -value;
        }
    }
}

fn main() {
    // Здесь можно поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_mutation() {
        // Клон происходит, потому что `input` нужно изменить.
        let vec = vec![-1, 0, 1];
        let mut input = Cow::from(&vec);
        abs_all(&mut input);
        assert!(matches!(input, Cow::Owned(_)));
    }

    #[test]
    fn reference_no_mutation() {
        // Клона нет, потому что `input` менять не нужно.
        let vec = vec![0, 1, 2];
        let mut input = Cow::from(&vec);
        abs_all(&mut input);
        // TODO: Замени `todo!()` на `Cow::Owned(_)` или `Cow::Borrowed(_)`.
        assert!(matches!(input, todo!()));
    }

    #[test]
    fn owned_no_mutation() {
        // Можно передать `vec` и без `&`, тогда `Cow` владеет им сразу. В этом
        // случае изменения не происходит (все числа уже неотрицательные), а значит
        // и клона нет. Но результат всё равно владеющий, потому что его
        // никогда не заимствовали и не меняли.
        let vec = vec![0, 1, 2];
        let mut input = Cow::from(vec);
        abs_all(&mut input);
        // TODO: Замени `todo!()` на `Cow::Owned(_)` или `Cow::Borrowed(_)`.
        assert!(matches!(input, todo!()));
    }

    #[test]
    fn owned_mutation() {
        // Конечно, так же и тогда, когда изменение происходит (не все
        // числа неотрицательные). В этом случае вызов `to_mut()` в функции
        // `abs_all` возвращает ссылку на те же данные, что были раньше.
        let vec = vec![-1, 0, 1];
        let mut input = Cow::from(vec);
        abs_all(&mut input);
        // TODO: Замени `todo!()` на `Cow::Owned(_)` или `Cow::Borrowed(_)`.
        assert!(matches!(input, todo!()));
    }
}
