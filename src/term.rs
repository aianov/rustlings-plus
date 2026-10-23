use crossterm::{
    Command, QueueableCommand,
    cursor::{self, MoveTo},
    style::{Attribute, Color, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{Clear, ClearType},
};
use std::{
    fmt, fs,
    io::{self, BufRead, StdoutLock, Write},
    panic, thread,
    time::{Duration, Instant},
};

use crate::{app_state::Status, i18n::tr};

pub struct MaxLenWriter<'a, 'lock> {
    pub stdout: &'a mut StdoutLock<'lock>,
    len: usize,
    max_len: usize,
}

impl<'a, 'lock> MaxLenWriter<'a, 'lock> {
    pub fn new(stdout: &'a mut StdoutLock<'lock>, max_len: usize) -> Self {
        Self {
            stdout,
            len: 0,
            max_len,
        }
    }

    // Additional is for emojis that take more space.
    pub fn add_to_len(&mut self, additional: usize) {
        self.len += additional;
    }
}

pub trait CountedWrite<'lock> {
    fn write_ascii(&mut self, ascii: &[u8]) -> io::Result<()>;
    fn write_str(&mut self, unicode: &str) -> io::Result<()>;
    fn stdout(&mut self) -> &mut StdoutLock<'lock>;
}

impl<'lock> CountedWrite<'lock> for MaxLenWriter<'_, 'lock> {
    fn write_ascii(&mut self, ascii: &[u8]) -> io::Result<()> {
        let n = ascii.len().min(self.max_len.saturating_sub(self.len));
        if n > 0 {
            self.stdout.write_all(&ascii[..n])?;
            self.len += n;
        }
        Ok(())
    }

    fn write_str(&mut self, unicode: &str) -> io::Result<()> {
        if let Some((ind, c)) = unicode
            .char_indices()
            .take(self.max_len.saturating_sub(self.len))
            .last()
        {
            self.stdout
                .write_all(&unicode.as_bytes()[..ind + c.len_utf8()])?;
            self.len += ind + 1;
        }

        Ok(())
    }

    fn stdout(&mut self) -> &mut StdoutLock<'lock> {
        self.stdout
    }
}

impl<'a> CountedWrite<'a> for StdoutLock<'a> {
    fn write_ascii(&mut self, ascii: &[u8]) -> io::Result<()> {
        self.write_all(ascii)
    }

    fn write_str(&mut self, unicode: &str) -> io::Result<()> {
        self.write_all(unicode.as_bytes())
    }

    fn stdout(&mut self) -> &mut StdoutLock<'a> {
        self
    }
}

pub struct ProgressCounter<'a, 'lock> {
    stdout: &'a mut StdoutLock<'lock>,
    total: usize,
    counter: usize,
}

impl<'a, 'lock> ProgressCounter<'a, 'lock> {
    pub fn new(stdout: &'a mut StdoutLock<'lock>, total: usize) -> io::Result<Self> {
        write!(stdout, "{}: 0/{total}", tr().progress)?;
        stdout.flush()?;

        Ok(Self {
            stdout,
            total,
            counter: 0,
        })
    }

    pub fn increment(&mut self) -> io::Result<()> {
        self.counter += 1;
        write!(
            self.stdout,
            "\r{}: {}/{}",
            tr().progress,
            self.counter,
            self.total
        )?;
        self.stdout.flush()
    }
}

impl Drop for ProgressCounter<'_, '_> {
    fn drop(&mut self) {
        let _ = self.stdout.write_all(b"\n\n");
    }
}

pub struct CheckProgressVisualizer<'a, 'lock>(ProgressCounter<'a, 'lock>);

impl<'a, 'lock> CheckProgressVisualizer<'a, 'lock> {
    pub fn build(stdout: &'a mut StdoutLock<'lock>, total: usize) -> io::Result<Self> {
        clear_terminal(stdout)?;
        stdout.write_all(tr().checking_all.as_bytes())?;
        stdout.write_all(b"\n")?;

        Ok(Self(ProgressCounter::new(stdout, total)?))
    }

    fn checked(&mut self, color: Color, label: &str, exercise_name: &str) -> io::Result<()> {
        self.0.stdout.queue(SetForegroundColor(color))?;
        self.0.stdout.write_all(b"\r")?;
        self.0.stdout.write_all(label.as_bytes())?;
        self.0.stdout.queue(ResetColor)?;
        self.0.stdout.write_all(exercise_name.as_bytes())?;
        self.0.stdout.queue(Clear(ClearType::UntilNewLine))?;

        self.0.stdout.write_all(b"\n")?;
        self.0.increment()
    }

    pub fn done(&mut self, exercise_name: &str) -> io::Result<()> {
        self.checked(Color::Green, tr().check_done, exercise_name)
    }

    pub fn pending(&mut self, exercise_name: &str) -> io::Result<()> {
        self.checked(Color::Red, tr().check_pending, exercise_name)
    }

    pub fn skipped(&mut self, exercise_name: &str) -> io::Result<()> {
        self.checked(Color::Yellow, tr().check_skip, exercise_name)
    }
}

pub fn progress_bar<'a>(
    writer: &mut impl CountedWrite<'a>,
    label: &str,
    progress: usize,
    total: usize,
    term_width: u16,
    status: Status,
) -> io::Result<()> {
    let counter = format!("] {progress:>3}/{total}");
    let prefix_width = label.chars().count() + 3;
    let wrapper_width = prefix_width + counter.len();
    let min_line_width = wrapper_width + 4;

    if usize::from(term_width) < min_line_width || total == 0 {
        writer.write_str(label)?;
        return writer.write_ascii(format!(": {progress}/{total}").as_bytes());
    }

    writer.write_str(label)?;
    writer.write_ascii(b": [")?;

    let width = usize::from(term_width) - wrapper_width;
    let filled = (width * progress) / total;
    let rest_color = match status {
        Status::Done | Status::Started => Color::Red,
        Status::Untouched => Color::DarkGrey,
    };

    let stdout = writer.stdout();
    stdout.queue(SetForegroundColor(Color::Green))?;
    let mut bar = String::with_capacity(width);
    for _ in 0..filled {
        bar.push('#');
    }
    stdout.write_all(bar.as_bytes())?;
    if filled < width {
        stdout.queue(SetForegroundColor(rest_color))?;
        let mut rest = String::with_capacity(width - filled);
        rest.push('>');
        for _ in 1..width - filled {
            rest.push('-');
        }
        stdout.write_all(rest.as_bytes())?;
    }
    stdout.queue(SetForegroundColor(Color::Reset))?;
    writer.write_ascii(counter.as_bytes())
}

const SNAKE_CELLS: usize = 3;
const SNAKE_LEN: usize = 6;
const SNAKE_TICK_MS: u128 = 70;

fn snake_path() -> Vec<(usize, usize)> {
    let w = SNAKE_CELLS * 2;
    let mut path = Vec::with_capacity(2 * w + 4);
    path.extend((0..w).map(|x| (x, 0)));
    path.extend((1..4).map(|y| (w - 1, y)));
    path.extend((0..w - 1).rev().map(|x| (x, 3)));
    path.extend((1..3).rev().map(|y| (0, y)));
    path
}

pub fn snake_frame(tick: usize) -> String {
    const BITS: [[u32; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];
    let path = snake_path();
    let mut cells = [0u32; SNAKE_CELLS];
    for i in 0..SNAKE_LEN {
        let (x, y) = path[(tick + i) % path.len()];
        cells[x / 2] |= BITS[x % 2][y];
    }
    cells
        .iter()
        .filter_map(|&c| char::from_u32(0x2800 + c))
        .collect()
}

pub fn with_snake<T: Send>(
    stdout: &mut StdoutLock,
    text: &str,
    job: impl FnOnce() -> T + Send,
) -> io::Result<T> {
    thread::scope(|s| {
        let handle = thread::Builder::new().spawn_scoped(s, job)?;
        stdout.queue(cursor::Hide)?;
        let started = Instant::now();
        let mut drawn = None;
        while !handle.is_finished() {
            let tick = (started.elapsed().as_millis() / SNAKE_TICK_MS) as usize;
            if drawn != Some(tick) {
                drawn = Some(tick);
                stdout.write_all(b"\r")?;
                stdout.queue(SetForegroundColor(Color::Magenta))?;
                stdout.write_all(snake_frame(tick).as_bytes())?;
                stdout.queue(ResetColor)?;
                write!(stdout, " {text}")?;
                stdout.flush()?;
            }
            thread::sleep(Duration::from_millis(10));
        }
        stdout.queue(cursor::Show)?;
        stdout.flush()?;
        Ok(handle.join().unwrap_or_else(|e| panic::resume_unwind(e)))
    })
}

pub fn clear_terminal(stdout: &mut StdoutLock) -> io::Result<()> {
    stdout
        .queue(MoveTo(0, 0))?
        .queue(Clear(ClearType::All))?
        .queue(Clear(ClearType::Purge))
        .map(|_| ())
}

pub fn press_enter_prompt(stdout: &mut StdoutLock) -> io::Result<()> {
    stdout.flush()?;
    io::stdin().lock().read_until(b'\n', &mut Vec::new())?;
    stdout.write_all(b"\n")
}

/// Canonicalize, convert to string and remove verbatim part on Windows.
pub fn canonicalize(path: &str) -> Option<String> {
    fs::canonicalize(path)
        .ok()?
        .into_os_string()
        .into_string()
        .ok()
        .map(|mut path| {
            // Windows itself can't handle its verbatim paths.
            if cfg!(windows) && path.as_bytes().starts_with(br"\\?\") {
                path.drain(..4);
            }

            path
        })
}

pub fn file_path<'a, W: CountedWrite<'a>>(
    writer: &mut W,
    color: Color,
    f: impl FnOnce(&mut W) -> io::Result<()>,
) -> io::Result<()> {
    writer
        .stdout()
        .queue(SetForegroundColor(color))?
        .queue(SetAttribute(Attribute::Underlined))?;

    f(writer)?;

    writer
        .stdout()
        .queue(SetForegroundColor(Color::Reset))?
        .queue(SetAttribute(Attribute::NoUnderline))?;

    Ok(())
}

pub fn terminal_file_link<'a>(
    writer: &mut impl CountedWrite<'a>,
    path: &str,
    canonical_path: &str,
) -> io::Result<()> {
    writer.stdout().write_all(b"\x1b]8;;file://")?;
    writer.stdout().write_all(canonical_path.as_bytes())?;
    writer.stdout().write_all(b"\x1b\\")?;
    // Only this part is visible.
    writer.write_str(path)?;
    writer.stdout().write_all(b"\x1b]8;;\x1b\\")
}

pub fn write_ansi(output: &mut Vec<u8>, command: impl Command) {
    struct FmtWriter<'a>(&'a mut Vec<u8>);

    impl fmt::Write for FmtWriter<'_> {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            self.0.extend_from_slice(s.as_bytes());
            Ok(())
        }
    }

    let _ = command.write_ansi(&mut FmtWriter(output));
}
