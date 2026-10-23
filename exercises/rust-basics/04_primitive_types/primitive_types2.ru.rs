// Символы (`char`)

fn main() {
    // Обрати внимание на _одинарные_ кавычки, они отличаются от двойных,
    // которые ты видел до сих пор.
    let my_first_initial = 'C';
    if my_first_initial.is_alphabetic() {
        println!("Буква!");
    } else if my_first_initial.is_numeric() {
        println!("Цифра!");
    } else {
        println!("Ни буква, ни цифра!");
    }

    // TODO: По аналогии с примером выше объяви переменную `your_character`
    // ниже со своим любимым символом.
    // Попробуй букву, цифру (в одинарных кавычках), специальный символ, символ
    // из другого языка, не твоего родного, или эмодзи 😉
    // let your_character = '';

    if your_character.is_alphabetic() {
        println!("Буква!");
    } else if your_character.is_numeric() {
        println!("Цифра!");
    } else {
        println!("Ни буква, ни цифра!");
    }
}
