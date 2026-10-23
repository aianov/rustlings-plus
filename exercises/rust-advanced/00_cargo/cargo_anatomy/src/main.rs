use std::process::ExitCode;

fn main() -> ExitCode {
    let expr = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    match tiny_calc::eval(&expr) {
        Some(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("не могу посчитать: {expr:?}");
            ExitCode::FAILURE
        }
    }
}
