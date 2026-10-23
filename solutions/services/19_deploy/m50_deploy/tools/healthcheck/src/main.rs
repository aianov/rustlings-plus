use std::io::Write;
use std::process::ExitCode;

use healthcheck::{PROBE_TIMEOUT, probe};

fn main() -> ExitCode {
    let Some(url) = std::env::args().nth(1) else {
        let _ = writeln!(std::io::stderr(), "healthcheck http://хост:порт/ready");
        return ExitCode::from(2);
    };
    match probe(&url, PROBE_TIMEOUT) {
        Ok(..) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "{url}: {error}");
            ExitCode::FAILURE
        }
    }
}
