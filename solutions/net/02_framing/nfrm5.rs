use bytes::BytesMut;

struct LineDecoder {
    buf: BytesMut,
}

impl LineDecoder {
    fn new() -> LineDecoder {
        LineDecoder { buf: BytesMut::new() }
    }

    fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(i) = self.buf.iter().position(|b| *b == b'\n') {
            let line = self.buf.split_to(i + 1);
            out.push(String::from_utf8_lossy(&line[..i]).into_owned());
        }
        out
    }
}

fn main() {
    let mut decoder = LineDecoder::new();
    for chunk in [&b"one\ntw"[..], b"o\nthree\n"] {
        println!("{:?}", decoder.push(chunk));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STREAM: &[u8] = b"alpha\nbeta\n\ngamma\ndelta\nepsilon";
    const EXPECTED: [&str; 5] = ["alpha", "beta", "", "gamma", "delta"];

    fn feed(chunks: &[&[u8]]) -> Vec<String> {
        let mut d = LineDecoder::new();
        chunks.iter().flat_map(|c| d.push(c)).collect()
    }

    #[test]
    fn whole_stream_at_once() {
        assert_eq!(feed(&[STREAM]), EXPECTED, "весь поток одним куском: потеряны сообщения, лежавшие в одном куске");
    }

    #[test]
    fn every_split_point_gives_the_same_result() {
        for k in 0..=STREAM.len() {
            let (a, b) = STREAM.split_at(k);
            assert_eq!(feed(&[a, b]), EXPECTED, "поток разрезан на позиции {k}: результат не должен зависеть от разреза");
        }
    }

    #[test]
    fn byte_by_byte() {
        let chunks: Vec<&[u8]> = STREAM.chunks(1).collect();
        assert_eq!(feed(&chunks), EXPECTED, "по одному байту");
    }

    #[test]
    fn three_way_splits() {
        for i in 0..STREAM.len() {
            for j in i..STREAM.len() {
                let chunks = [&STREAM[..i], &STREAM[i..j], &STREAM[j..]];
                assert_eq!(feed(&chunks), EXPECTED, "разрезы на позициях {i} и {j}");
            }
        }
    }
}
