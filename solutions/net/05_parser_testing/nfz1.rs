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

struct Echo;

impl Decode for Echo {
    fn feed(&mut self, chunk: &[u8]) -> Vec<String> {
        vec![String::from_utf8_lossy(chunk).into_owned()]
    }
}

fn main() {
    println!("{:?}", check_splits(b"ab\ncd\n", || Echo).is_err());
}

#[cfg(test)]
mod tests {
    use super::*;

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
            for &b in chunk {
                if b == b'\n' {
                    let mut line = std::mem::take(&mut self.buf);
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    out.push(String::from_utf8_lossy(&line).into_owned());
                } else {
                    self.buf.push(b);
                }
            }
            out
        }
    }

    struct StripsTrailingCr(LineDecoder);

    impl Decode for StripsTrailingCr {
        fn feed(&mut self, chunk: &[u8]) -> Vec<String> {
            self.0.feed(chunk.strip_suffix(b"\r").unwrap_or(chunk))
        }
    }

    struct DropsLoneCrChunk(LineDecoder);

    impl Decode for DropsLoneCrChunk {
        fn feed(&mut self, chunk: &[u8]) -> Vec<String> {
            if chunk == b"\r" {
                return Vec::new();
            }
            self.0.feed(chunk)
        }
    }

    #[test]
    fn correct_decoder_passes_on_everything() {
        for input in [&b""[..], b"x", b"\n", b"one\r\ntwo\n", b"a\rb\r\nc", b"\r\n\r\n"] {
            let res = check_splits(input, LineDecoder::new);
            assert!(res.is_ok(), "верный декодер не должен ловиться: {res:?}");
        }
    }

    #[test]
    fn finds_a_bug_that_a_single_cut_exposes() {
        let input = b"a\rb\r\n";
        let fail = check_splits(input, || StripsTrailingCr(LineDecoder::new())).expect_err("декодер теряет '\\r' на конце куска: при разрезе после 'a\\r' ответ меняется");
        assert_eq!(fail.cuts.len(), 1, "достаточно одного разреза, а в отчёте {:?}: сначала перебирай разрезы на два куска", fail.cuts);
        assert_ne!(fail.whole, fail.split);
        assert_eq!(fail.whole, vec!["a\rb".to_string()]);
    }

    #[test]
    fn finds_a_bug_only_byte_by_byte_exposes() {
        let input = b"a\rb\r\n";
        let fail = check_splits(input, || DropsLoneCrChunk(LineDecoder::new())).expect_err("этот баг виден только когда '\\r' приходит отдельным куском: два куска его не покажут, нужна побайтовая подача");
        assert!(fail.cuts.len() > 1, "ожидался побайтовый разрез, получено {:?}", fail.cuts);
    }

    #[test]
    fn failure_can_be_replayed_from_cuts() {
        let input = b"x\ry\n";
        let fail = check_splits(input, || StripsTrailingCr(LineDecoder::new())).unwrap_err();
        let again = run_chunks(StripsTrailingCr(LineDecoder::new()), &chunks_at(input, &fail.cuts));
        assert_eq!(again, fail.split, "по разрезам из отчёта сбой должен воспроизводиться");
        assert_ne!(again, fail.whole);
    }
}
