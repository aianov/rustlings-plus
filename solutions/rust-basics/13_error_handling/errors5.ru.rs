// Это слегка изменённая версия упражнения `errors4`. В ней есть понятия,
// до которых мы дойдём позже, например `Box` и трейт `From`. Сейчас разбираться
// в них подробно не нужно, но можешь заглянуть вперёд. Пока думай о типе
// `Box<dyn ???>` как о типе «мне подойдёт всё, что умеет ???».
//
// Коротко: такой `Box` нужен, когда ты хочешь владеть значением, а важно
// лишь то, что его тип реализует определённый трейт. Для этого `Box`
// объявляется как `Box<dyn Trait>`, где `Trait` это трейт, который
// компилятор ищет у любого значения в этом контексте. В этом
// упражнении контекст это возможные
// ошибки, которые можно вернуть
// из `Result`.

use std::error::Error;
use std::fmt;

#[derive(PartialEq, Debug)]
enum CreationError {
    Negative,
    Zero,
}

// Это нужно, чтобы `CreationError` мог реализовать `Error`.
impl fmt::Display for CreationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let description = match *self {
            CreationError::Negative => "число отрицательное",
            CreationError::Zero => "число равно нулю",
        };
        f.write_str(description)
    }
}

impl Error for CreationError {}

#[derive(PartialEq, Debug)]
struct PositiveNonzeroInteger(u64);

impl PositiveNonzeroInteger {
    fn new(value: i64) -> Result<PositiveNonzeroInteger, CreationError> {
        match value {
            x if x < 0 => Err(CreationError::Negative),
            0 => Err(CreationError::Zero),
            x => Ok(PositiveNonzeroInteger(x as u64)),
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let pretend_user_input = "42";
    let x: i64 = pretend_user_input.parse()?;
    println!("вывод={:?}", PositiveNonzeroInteger::new(x)?);
    Ok(())
}
