// Построчный декодер кормят кусками. На целом входе он работает, а тест перебора разрезов (готовый
// check_splits, как в предыдущей задаче) краснеет на строках, где одинокий '\r' стоит внутри строки.
// Ручной тест этого не ловил, потому что автор резал вход только посреди слов.
// TODO: Запусти тесты, посмотри, какие разрезы падают, и почини LineDecoder. Строка кончается на "\n";
// один '\r' непосредственно перед ним не входит в строку, любые другие '\r' остаются в ней.

trait Decode {
    fn feed(&mut self, chunk: &[u8]) -> Vec<String>;
}

fn run_chunks<D: Decode>(mut decoder: D, chunks: &[&[u8]]) -> Vec<String> {
    chunks.iter().flat_map(|c| decoder.feed(c)).collect()
}

fn chunks_at<'a>(input: &'a [u8], cuts: &[usize]) -> Vec<&'a [u8]> {
    let mut out = Vec::new();
    let mut prev = 0;
    for &cut in cuts {
        out.push(&input[prev..cut]);
        prev = cut;
    }
    out.push(&input[prev..]);
    out
}
#[derive(Debug)]
struct SplitFailure {
    cuts: Vec<usize>,
    whole: Vec<String>,
    split: Vec<String>,
}

fn check_splits<D: Decode>(input: &[u8], make: impl Fn() -> D) -> Result<(), SplitFailure> {
    let whole = run_chunks(make(), &[input]);
    let mut attempts: Vec<Vec<usize>> = (1..input.len()).map(|cut| vec![cut]).collect();
    attempts.push((1..input.len()).collect());
    for cuts in attempts {
        let split = run_chunks(make(), &chunks_at(input, &cuts));
        if split != whole {
            return Err(SplitFailure { cuts, whole, split });
        }
    }
    Ok(())
}

struct LineDecoder {
    buf: Vec<u8>,
}

impl LineDecoder {
    fn new() -> LineDecoder {
        LineDecoder { buf: Vec::new() }
    }
}

impl Decode for LineDecoder {
    fn feed(&mut self, chunk: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        for (i, &b) in chunk.iter().enumerate() {
            if b == b'\r' && i + 1 == chunk.len() {
                continue;
            }
            if b == b'\n' {
                let line = std::mem::take(&mut self.buf);
                out.push(String::from_utf8_lossy(&line).into_owned());
            } else {
                self.buf.push(b);
            }
        }
        out
    }
}

fn main() {
    let mut d = LineDecoder::new();
    println!("{:?}", d.feed(b"hello\r\nworld\r\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_input_examples() {
        let mut d = LineDecoder::new();
        assert_eq!(d.feed(b"a\rb\r\nc\n\r\nlast"), vec!["a\rb", "c", ""]);
        assert_eq!(d.feed(b"\r\n"), vec!["last"]);
    }

    #[test]
    fn every_split_agrees() {
        let inputs: [&[u8]; 7] = [b"a\rb\r\nc\r\n", b"\r\r\n", b"x\r\ry\n", b"\r", b"\n\r\n\r", b"one\ntwo\r\nthree\r\r\n", b"\r\rend\r\n"];
        for input in inputs {
            if let Err(f) = check_splits(input, LineDecoder::new) {
                panic!("разрезы {:?} дали другой результат: целиком {:?}, по кускам {:?}; вход {:?}", f.cuts, f.whole, f.split, String::from_utf8_lossy(input));
            }
        }
    }

    #[test]
    fn lone_cr_stays_inside_the_line() {
        let mut d = LineDecoder::new();
        assert!(d.feed(b"a\r").is_empty());
        assert_eq!(d.feed(b"b\n"), vec!["a\rb"], "строка вышла не такой, какой вышла бы при подаче целиком");
    }

    #[test]
    fn cr_then_lf_across_chunks() {
        let mut d = LineDecoder::new();
        assert!(d.feed(b"line\r").is_empty());
        assert_eq!(d.feed(b"\n"), vec!["line"]);
    }
}
