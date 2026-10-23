
use std::io;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::os::unix::process::CommandExt;

fn main() {}

pub struct Spawned {
    pub pid: u32,
    pub stdout: ChildStdout,
}

#[derive(Default)]
pub struct Launcher {
    children: Vec<Child>,
}

impl Launcher {
    pub fn new() -> Launcher {
        Launcher::default()
    }

    pub fn spawn(&mut self, program: &str, args: &[&str]) -> io::Result<Spawned> {
        let mut child = Command::new(program).args(args).stdout(Stdio::piped()).process_group(0).spawn()?;
        let stdout = child.stdout.take().expect("stdout запрошен как piped");
        let pid = child.id();
        self.children.push(child);
        Ok(Spawned { pid, stdout })
    }
}

impl Drop for Launcher {
    fn drop(&mut self) {
        for child in &mut self.children {
            let _ = Command::new("kill").args(["-KILL", "--", &format!("-{}", child.id())]).stderr(Stdio::null()).status();
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::time::{Duration, Instant};

    fn alive(pid: u32) -> bool {
        Command::new("kill").args(["-0", &pid.to_string()]).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
    }

    fn gone_within(pid: u32, limit: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < limit {
            if !alive(pid) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        !alive(pid)
    }

    #[test]
    fn children_do_not_outlive_the_launcher() {
        let mut l = Launcher::new();
        let a = l.spawn("sleep", &["20"]).unwrap().pid;
        let b = l.spawn("sleep", &["20"]).unwrap().pid;
        assert!(alive(a) && alive(b));
        drop(l);
        assert!(gone_within(a, Duration::from_secs(3)), "процесс {a} пережил лаунчер: остался сиротой или зомби (его надо подобрать, `wait`)");
        assert!(gone_within(b, Duration::from_secs(3)), "процесс {b} пережил лаунчер");
    }

    #[test]
    fn grandchildren_die_too() {
        let mut l = Launcher::new();
        let Spawned { pid, stdout } = l.spawn("sh", &["-c", "sleep 20 & echo $!; wait"]).unwrap();
        let mut line = String::new();
        BufReader::new(stdout).read_line(&mut line).unwrap();
        let grandchild: u32 = line.trim().parse().expect("sh должен напечатать номер внука");
        assert!(alive(grandchild));
        drop(l);
        assert!(gone_within(pid, Duration::from_secs(3)), "оболочка пережила лаунчер");
        assert!(gone_within(grandchild, Duration::from_secs(3)), "процесс {grandchild}, порождённый сервисом, остался жить: убить надо группу процессов целиком");
    }

    #[test]
    fn a_panic_cleans_up_too() {
        let (tx, rx) = std::sync::mpsc::channel();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut l = Launcher::new();
            let pid = l.spawn("sleep", &["20"]).unwrap().pid;
            tx.send(pid).unwrap();
            panic!("лаунчер упал");
        }));
        assert!(result.is_err());
        let pid = rx.recv().unwrap();
        assert!(gone_within(pid, Duration::from_secs(3)), "после паники процесс {pid} остался жить");
    }

    #[test]
    fn already_finished_children_are_not_a_problem() {
        let mut l = Launcher::new();
        let pid = l.spawn("true", &[]).unwrap().pid;
        std::thread::sleep(Duration::from_millis(200));
        drop(l);
        assert!(gone_within(pid, Duration::from_secs(1)));
    }
}
