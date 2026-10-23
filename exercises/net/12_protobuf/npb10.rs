// Свой кодировщик ничего не стоит, пока не доказано, что он пишет то же самое, что и настоящая библиотека. Сверка по байтам
// лучший способ: берём тысячи случайных сообщений, кодируем сами и через prost и требуем побайтного совпадения. Любое расхождение
// это баг в нашем кодировщике (или в нашем понимании формата). TODO: Допиши encode_plain: Plain это обычная структура без prost,
// схема в таблице ниже. Помогают готовые put_varint и put_key. Поля идут по возрастанию номеров, значения по умолчанию не пишутся.
//   1 id u64 | 2 delta sint32 | 3 name string | 4 blob bytes | 5 ratio double | 6 tags repeated uint32 (упакованно)
//   7 flag bool | 8 inner message { 1 x int64 } | 9 signed int64 | 10 small int32 | 11 f float | 12 fx fixed32

#![allow(dead_code)]

#[derive(Debug, Clone, Default, PartialEq)]
struct InnerPlain {
    x: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Plain {
    id: u64,
    delta: i32,
    name: String,
    blob: Vec<u8>,
    ratio: f64,
    tags: Vec<u32>,
    flag: bool,
    inner: Option<InnerPlain>,
    signed: i64,
    small: i32,
    f: f32,
    fx: u32,
}

fn put_varint(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let low = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(low);
            return;
        }
        out.push(low | 0x80);
    }
}

const VARINT: u32 = 0;
const I64: u32 = 1;
const LEN: u32 = 2;
const I32: u32 = 5;

fn put_key(number: u32, wire: u32, out: &mut Vec<u8>) {
    put_varint(u64::from((number << 3) | wire), out);
}

fn encode_plain(p: &Plain) -> Vec<u8> {
    let _ = p;
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;

    #[derive(Clone, PartialEq, Message)]
    struct WireInner {
        #[prost(int64, tag = "1")]
        x: i64,
    }

    #[derive(Clone, PartialEq, Message)]
    struct Wire {
        #[prost(uint64, tag = "1")]
        id: u64,
        #[prost(sint32, tag = "2")]
        delta: i32,
        #[prost(string, tag = "3")]
        name: String,
        #[prost(bytes = "vec", tag = "4")]
        blob: Vec<u8>,
        #[prost(double, tag = "5")]
        ratio: f64,
        #[prost(uint32, repeated, tag = "6")]
        tags: Vec<u32>,
        #[prost(bool, tag = "7")]
        flag: bool,
        #[prost(message, optional, tag = "8")]
        inner: Option<WireInner>,
        #[prost(int64, tag = "9")]
        signed: i64,
        #[prost(int32, tag = "10")]
        small: i32,
        #[prost(float, tag = "11")]
        f: f32,
        #[prost(fixed32, tag = "12")]
        fx: u32,
    }

    fn reference(p: &Plain) -> Vec<u8> {
        Wire {
            id: p.id,
            delta: p.delta,
            name: p.name.clone(),
            blob: p.blob.clone(),
            ratio: p.ratio,
            tags: p.tags.clone(),
            flag: p.flag,
            inner: p.inner.as_ref().map(|i| WireInner { x: i.x }),
            signed: p.signed,
            small: p.small,
            f: p.f,
            fx: p.fx,
        }
        .encode_to_vec()
    }

    struct Rng(u64);

    impl Rng {
        fn step(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 >> 11
        }

        fn below(&mut self, n: u64) -> u64 {
            self.step() % n
        }

        fn big(&mut self) -> u64 {
            match self.below(6) {
                0 => 0,
                1 => u64::MAX,
                2 => 1 << self.below(64),
                _ => self.step() >> self.below(60),
            }
        }
    }

    fn random_plain(rng: &mut Rng) -> Plain {
        let mut p = Plain::default();
        if rng.below(3) != 0 {
            p.id = rng.big();
        }
        if rng.below(3) != 0 {
            p.delta = [0, 1, -1, i32::MAX, i32::MIN, rng.step() as i32][rng.below(6) as usize];
        }
        if rng.below(3) != 0 {
            p.name = (0..rng.below(12)).map(|_| ['a', 'ё', 'я', '😀', ' '][rng.below(5) as usize]).collect();
        }
        if rng.below(3) != 0 {
            p.blob = (0..rng.below(200)).map(|_| rng.step() as u8).collect();
        }
        if rng.below(3) != 0 {
            p.ratio = (rng.step() as i32) as f64 / 7.0;
        }
        if rng.below(3) != 0 {
            p.tags = (0..rng.below(9)).map(|_| rng.big() as u32).collect();
        }
        p.flag = rng.below(2) == 0;
        match rng.below(3) {
            0 => {}
            1 => p.inner = Some(InnerPlain::default()),
            _ => p.inner = Some(InnerPlain { x: rng.big() as i64 }),
        }
        if rng.below(3) != 0 {
            p.signed = rng.big() as i64;
        }
        if rng.below(3) != 0 {
            p.small = [0, -1, i32::MIN, i32::MAX, rng.step() as i32][rng.below(5) as usize];
        }
        if rng.below(3) != 0 {
            p.f = (rng.step() as i32) as f32 / 3.0;
        }
        if rng.below(3) != 0 {
            p.fx = rng.big() as u32;
        }
        p
    }

    #[test]
    fn empty_message_is_empty() {
        assert!(encode_plain(&Plain::default()).is_empty());
    }

    #[test]
    fn present_but_empty_inner_message_is_written() {
        let p = Plain { inner: Some(InnerPlain::default()), ..Plain::default() };
        assert_eq!(encode_plain(&p), vec![0x42, 0x00], "заданное пустое вложенное сообщение это ключ и нулевая длина");
    }

    #[test]
    fn each_field_alone_matches_prost() {
        let samples = [
            Plain { id: 150, ..Plain::default() },
            Plain { delta: -1, ..Plain::default() },
            Plain { name: "Привет".into(), ..Plain::default() },
            Plain { blob: vec![0, 255], ..Plain::default() },
            Plain { ratio: -1.5, ..Plain::default() },
            Plain { tags: vec![1, 300, u32::MAX], ..Plain::default() },
            Plain { flag: true, ..Plain::default() },
            Plain { inner: Some(InnerPlain { x: -1 }), ..Plain::default() },
            Plain { signed: i64::MIN, ..Plain::default() },
            Plain { small: -5, ..Plain::default() },
            Plain { f: 2.5, ..Plain::default() },
            Plain { fx: 0xdead_beef, ..Plain::default() },
        ];
        for p in samples {
            assert_eq!(encode_plain(&p), reference(&p), "расхождение с prost на {p:?}");
        }
    }

    #[test]
    fn random_messages_match_prost_byte_for_byte() {
        let mut rng = Rng(0x5eed);
        for round in 0..4000 {
            let p = random_plain(&mut rng);
            let ours = encode_plain(&p);
            let theirs = reference(&p);
            assert_eq!(ours, theirs, "раунд {round}: расхождение с prost на {p:?}");
            let decoded = Wire::decode(&ours[..]).expect("prost не смог прочитать наши байты");
            assert_eq!(decoded.encode_to_vec(), ours);
        }
    }
}
