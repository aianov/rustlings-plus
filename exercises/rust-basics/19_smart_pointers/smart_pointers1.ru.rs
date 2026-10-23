// Во время компиляции Rust должен знать, сколько места занимает тип. С
// рекурсивными типами это проблема: значение может содержать внутри себя
// другое значение того же типа. Чтобы обойти это, можно использовать
// `Box`, умный указатель, который хранит данные в куче и позволяет
// обернуть рекурсивный тип.
//
// Рекурсивный тип в этом упражнении это «cons-список», структура данных,
// часто встречающаяся в функциональных языках. Каждый элемент cons-списка
// содержит два значения: значение текущего элемента и следующий
// элемент. Последний элемент это значение `Nil`.

// TODO: Используй `Box` в определении enum, чтобы код скомпилировался.
#[derive(PartialEq, Debug)]
enum List {
    Cons(i32, List),
    Nil,
}

// TODO: Создай пустой cons-список.
fn create_empty_list() -> List {
    todo!()
}

// TODO: Создай непустой cons-список.
fn create_non_empty_list() -> List {
    todo!()
}

fn main() {
    println!("Это пустой cons-список: {:?}", create_empty_list());
    println!(
        "Это непустой cons-список: {:?}",
        create_non_empty_list(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_empty_list() {
        assert_eq!(create_empty_list(), List::Nil);
    }

    #[test]
    fn test_create_non_empty_list() {
        assert_ne!(create_empty_list(), create_non_empty_list());
    }
}
