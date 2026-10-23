use std::process::Command;

pub struct Step {
    pub name: &'static str,
    pub program: &'static str,
    pub args: Vec<&'static str>,
}

fn run_step(step: &Step) -> Result<(), String> {
    let status = Command::new(step.program)
        .args(&step.args)
        .status()
        .map_err(|e| format!("step `{}` could not start: {e}", step.name))?;
    if status.success() {
        return Ok(());
    }
    match status.code() {
        Some(code) => Err(format!("step `{}` failed with code {code}", step.name)),
        None => Err(format!("step `{}` was killed by a signal", step.name)),
    }
}

pub fn run_steps(steps: &[Step]) -> Result<Vec<&'static str>, String> {
    let mut passed = Vec::new();
    for step in steps {
        run_step(step)?;
        println!("ok: {}", step.name);
        passed.push(step.name);
    }
    Ok(passed)
}

fn main() {
    let steps = vec![Step { name: "version", program: "cargo", args: vec!["--version"] }];
    match run_steps(&steps) {
        Ok(done) => println!("all passed: {done:?}"),
        Err(message) => eprintln!("{message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(name: &'static str, script: &'static str) -> Step {
        Step { name, program: "sh", args: vec!["-c", script] }
    }

    #[test]
    fn all_green_returns_every_name() {
        let steps = [sh("one", "exit 0"), sh("two", "true")];
        assert_eq!(run_steps(&steps), Ok(vec!["one", "two"]), "оба шага успешны, оба должны попасть в результат");
    }

    #[test]
    fn failing_step_is_an_error_with_name_and_code() {
        let steps = [sh("fine", "exit 0"), sh("broken", "exit 7")];
        let error = run_steps(&steps).expect_err("упавший шаг должен давать Err");
        assert!(error.contains("broken"), "в сообщении нужно имя шага: {error}");
        assert!(error.contains('7'), "в сообщении нужен код возврата 7: {error}");
        assert!(!error.contains("fine"), "успешный шаг в ошибку попадать не должен: {error}");
    }

    #[test]
    fn nothing_runs_after_the_first_failure() {
        let marker = std::env::temp_dir().join(format!("tool4_marker_{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let script: &'static str = Box::leak(format!("touch {}", marker.display()).into_boxed_str());
        let steps = [sh("broken", "exit 1"), sh("later", script)];
        assert!(run_steps(&steps).is_err());
        assert!(!marker.exists(), "шаг после упавшего всё равно выполнился: цепочка должна останавливаться на первой ошибке");
        let _ = std::fs::remove_file(&marker);
    }

    #[test]
    fn missing_program_is_a_failure_too() {
        let steps = [Step { name: "ghost", program: "no-such-program-for-the-course", args: vec![] }];
        let error = run_steps(&steps).expect_err("программа, которой нет, это тоже провал шага");
        assert!(error.contains("ghost"), "в сообщении нужно имя шага: {error}");
    }

    #[test]
    fn empty_chain_is_fine() {
        assert_eq!(run_steps(&[]), Ok(vec![]));
    }
}
