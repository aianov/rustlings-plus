// При работе с элементами коллекции итераторы незаменимы. Этот модуль поможет
// освоиться со структурой использования итератора и с проходом по элементам
// итерируемой коллекции.

fn main() {
    // Здесь можно поэкспериментировать.
}

#[cfg(test)]
mod tests {
    #[test]
    fn iterators() {
        let my_fav_fruits = &["banana", "custard apple", "avocado", "peach", "raspberry"];

        // Создаём итератор по срезу.
        let mut fav_fruits_iterator = my_fav_fruits.iter();

        assert_eq!(fav_fruits_iterator.next(), Some(&"banana"));
        assert_eq!(fav_fruits_iterator.next(), Some(&"custard apple"));
        assert_eq!(fav_fruits_iterator.next(), Some(&"avocado"));
        assert_eq!(fav_fruits_iterator.next(), Some(&"peach"));
        assert_eq!(fav_fruits_iterator.next(), Some(&"raspberry"));
        assert_eq!(fav_fruits_iterator.next(), None);
        //                                     ^^^^ дошли до конца
    }
}
