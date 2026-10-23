struct ColorRegularStruct {
    // TODO: Добавь поля, которых ждёт тест `regular_structs`.
    // Какого типа должны быть поля? Каковы минимальное и максимальное значения для цветов RGB?
}

struct ColorTupleStruct(/* TODO: Добавь поля, которых ждёт тест `tuple_structs` */);

#[derive(Debug)]
struct UnitStruct;

fn main() {
    // Здесь можно по желанию поэкспериментировать.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_structs() {
        // TODO: Создай экземпляр обычной структуры.
        // let green =

        assert_eq!(green.red, 0);
        assert_eq!(green.green, 255);
        assert_eq!(green.blue, 0);
    }

    #[test]
    fn tuple_structs() {
        // TODO: Создай экземпляр кортежной структуры.
        // let green =

        assert_eq!(green.0, 0);
        assert_eq!(green.1, 255);
        assert_eq!(green.2, 0);
    }

    #[test]
    fn unit_structs() {
        // TODO: Создай экземпляр unit-структуры.
        // let unit_struct =
        let message = format!("{unit_struct:?}s are fun!");

        assert_eq!(message, "UnitStructs are fun!");
    }
}
