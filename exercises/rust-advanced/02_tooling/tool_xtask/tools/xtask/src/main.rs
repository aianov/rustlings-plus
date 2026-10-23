// Здесь будут жить скрипты разработчика. Подкоманды и правила описаны в README.md.
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    println!("xtask пока ничего не умеет: {args:?}");
    ExitCode::SUCCESS
}
