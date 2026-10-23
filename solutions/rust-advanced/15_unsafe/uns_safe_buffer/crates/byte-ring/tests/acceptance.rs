use byte_ring::{ByteRing, Full};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const SRC: &str = include_str!("../src/lib.rs");

fn within<T: Send + 'static>(secs: u64, what: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(Duration::from_secs(secs)) {
        Ok(value) => value,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!("{what}: не уложилось в {secs} с, похоже на зависание"),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => panic!("{what}: поток упал с паникой"),
    }
}

#[test]
fn bytes_come_out_in_the_order_they_went_in() {
    let mut ring = ByteRing::with_capacity(3);
    assert!(ring.is_empty() && !ring.is_full());
    assert_eq!(ring.capacity(), 3);
    assert_eq!(ring.pop(), None);
    assert_eq!(ring.peek(), None);
    for byte in [10, 20, 30] {
        assert_eq!(ring.push(byte), Ok(()));
    }
    assert!(ring.is_full());
    assert_eq!(ring.len(), 3);
    assert_eq!(ring.push(40), Err(Full), "в полный буфер класть нельзя");
    assert_eq!(ring.peek(), Some(10));
    assert_eq!((ring.pop(), ring.pop(), ring.pop(), ring.pop()), (Some(10), Some(20), Some(30), None));
    assert_eq!(ring.push(50), Ok(()), "после освобождения места снова можно");
}

#[test]
fn a_zero_capacity_ring_is_always_empty_and_full() {
    let mut ring = ByteRing::with_capacity(0);
    assert_eq!(ring.capacity(), 0);
    assert!(ring.is_empty() && ring.is_full());
    assert_eq!(ring.push(1), Err(Full));
    assert_eq!(ring.pop(), None);
    assert_eq!(ring.peek(), None);
    assert_eq!(ring.write(&[1, 2, 3]), 0);
    let mut out = [0_u8; 4];
    assert_eq!(ring.read(&mut out), 0);
    assert_eq!(ring.discard(5), 0);
    let (a, b) = ring.as_slices();
    assert!(a.is_empty() && b.is_empty());
    ring.clear();
}

#[test]
fn data_that_crosses_the_seam_comes_back_as_two_slices() {
    let mut ring = ByteRing::with_capacity(8);
    assert_eq!(ring.write(&[0, 1, 2, 3, 4, 5, 6, 7]), 8);
    let mut first = [0_u8; 5];
    assert_eq!(ring.read(&mut first), 5);
    assert_eq!(first, [0, 1, 2, 3, 4]);
    assert_eq!(ring.write(&[8, 9, 10, 11, 12]), 5);
    assert_eq!(ring.len(), 8);
    let (a, b) = ring.as_slices();
    assert_eq!(a, [5, 6, 7], "первый кусок идёт до конца памяти");
    assert_eq!(b, [8, 9, 10, 11, 12], "второй начинается с начала памяти");
    let mut all = [0_u8; 8];
    assert_eq!(ring.read(&mut all), 8);
    assert_eq!(all, [5, 6, 7, 8, 9, 10, 11, 12]);
    assert!(ring.is_empty());
}

#[test]
fn unwrapped_data_has_an_empty_second_slice() {
    let mut ring = ByteRing::with_capacity(8);
    ring.write(&[1, 2, 3]);
    let (a, b) = ring.as_slices();
    assert_eq!((a, b), (&[1_u8, 2, 3][..], &[][..]));
}

#[test]
fn writes_are_partial_when_there_is_no_room() {
    let mut ring = ByteRing::with_capacity(4);
    assert_eq!(ring.write(&[1, 2, 3, 4, 5, 6]), 4, "влезло четыре байта");
    assert_eq!(ring.write(&[9]), 0);
    let mut out = [0_u8; 2];
    assert_eq!(ring.read(&mut out), 2);
    assert_eq!(out, [1, 2]);
    assert_eq!(ring.write(&[7, 8, 9]), 2);
    let mut rest = [0_u8; 10];
    assert_eq!(ring.read(&mut rest), 4, "читается не больше, чем лежит");
    assert_eq!(&rest[..4], &[3, 4, 7, 8]);
    assert_eq!(ring.read(&mut rest), 0);
}

#[test]
fn discard_throws_away_the_oldest_bytes() {
    let mut ring = ByteRing::with_capacity(5);
    ring.write(&[1, 2, 3, 4, 5]);
    assert_eq!(ring.discard(2), 2);
    assert_eq!(ring.peek(), Some(3));
    ring.write(&[6, 7]);
    assert_eq!(ring.discard(100), 5, "выбросить можно не больше, чем лежит");
    assert!(ring.is_empty());
    ring.write(&[8, 9]);
    ring.clear();
    assert!(ring.is_empty());
    assert_eq!(ring.write(&[1, 2, 3, 4, 5]), 5, "после clear вся ёмкость свободна");
}

#[test]
fn random_operations_agree_with_a_vecdeque_model() {
    for capacity in 1..=17_usize {
        let mut ring = ByteRing::with_capacity(capacity);
        let mut model: VecDeque<u8> = VecDeque::new();
        let mut seed = capacity as u64 * 7919 + 1;
        for step in 0..3000 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let operation = (seed >> 33) % 7;
            let byte = (seed >> 41) as u8;
            let amount = ((seed >> 20) % (capacity as u64 + 3)) as usize;
            match operation {
                0 => {
                    let result = ring.push(byte);
                    if model.len() < capacity {
                        assert_eq!(result, Ok(()), "ёмкость {capacity}, шаг {step}: push в непустое место отказал");
                        model.push_back(byte);
                    } else {
                        assert_eq!(result, Err(Full), "ёмкость {capacity}, шаг {step}: push в полный буфер прошёл");
                    }
                }
                1 => assert_eq!(ring.pop(), model.pop_front(), "ёмкость {capacity}, шаг {step}: pop"),
                2 => {
                    let data: Vec<u8> = (0..amount).map(|i| byte.wrapping_add(i as u8)).collect();
                    let written = ring.write(&data);
                    assert_eq!(written, amount.min(capacity - model.len()), "ёмкость {capacity}, шаг {step}: write вернул не то число");
                    model.extend(&data[..written]);
                }
                3 => {
                    let mut out = vec![0_u8; amount];
                    let got = ring.read(&mut out);
                    assert_eq!(got, amount.min(model.len()), "ёмкость {capacity}, шаг {step}: read вернул не то число");
                    let expected: Vec<u8> = model.drain(..got).collect();
                    assert_eq!(&out[..got], &expected[..], "ёмкость {capacity}, шаг {step}: read отдал не те байты");
                }
                4 => {
                    let dropped = ring.discard(amount);
                    assert_eq!(dropped, amount.min(model.len()));
                    model.drain(..dropped);
                }
                5 => assert_eq!(ring.peek(), model.front().copied(), "ёмкость {capacity}, шаг {step}: peek"),
                _ => {
                    if seed % 29 == 0 {
                        ring.clear();
                        model.clear();
                    }
                }
            }
            assert_eq!(ring.len(), model.len(), "ёмкость {capacity}, шаг {step}: len");
            assert_eq!(ring.is_full(), model.len() == capacity);
            let (a, b) = ring.as_slices();
            let joined = [a, b].concat();
            assert_eq!(joined, model.iter().copied().collect::<Vec<u8>>(), "ёмкость {capacity}, шаг {step}: as_slices");
            assert!(!a.is_empty() || b.is_empty(), "второй срез при пустом первом должен быть пуст");
        }
    }
}

#[test]
fn the_ring_can_be_moved_to_another_thread() {
    let out = within(5, "перенос в поток", || {
        let mut ring = ByteRing::with_capacity(4);
        ring.write(&[1, 2, 3]);
        thread::spawn(move || {
            let mut out = [0_u8; 3];
            ring.read(&mut out);
            out
        })
        .join()
        .unwrap()
    });
    assert_eq!(out, [1, 2, 3]);
}

#[test]
fn a_producer_and_a_consumer_share_the_ring_behind_a_mutex() {
    const TOTAL: usize = 100_000;
    let (received, in_order) = within(20, "производитель и потребитель", || {
        let ring = Arc::new(Mutex::new(ByteRing::with_capacity(61)));
        let producer_ring = Arc::clone(&ring);
        let producer = thread::spawn(move || {
            let data: Vec<u8> = (0..TOTAL).map(|i| (i % 251) as u8).collect();
            let mut sent = 0;
            while sent < TOTAL {
                let end = (sent + 17).min(TOTAL);
                let written = producer_ring.lock().unwrap().write(&data[sent..end]);
                if written == 0 {
                    thread::yield_now();
                }
                sent += written;
            }
        });
        let mut received = 0;
        let mut in_order = true;
        let mut chunk = [0_u8; 23];
        while received < TOTAL {
            let got = ring.lock().unwrap().read(&mut chunk);
            if got == 0 {
                thread::yield_now();
            }
            for byte in &chunk[..got] {
                if *byte != (received % 251) as u8 {
                    in_order = false;
                }
                received += 1;
            }
        }
        producer.join().unwrap();
        (received, in_order)
    });
    assert_eq!(received, TOTAL);
    assert!(in_order, "байты пришли в другом порядке или испорчены");
}

#[test]
fn the_unsafe_code_is_documented_and_the_api_is_safe() {
    let code = SRC.split("#[cfg(test)]").next().unwrap_or("");
    let blocks = code.matches("unsafe {").count() + code.matches("unsafe impl").count();
    let reasons = code.matches("// SAFETY:").count();
    assert!(blocks > 0, "внутри буфера должен остаться unsafe: прямая работа с памятью это и есть задача");
    assert!(reasons >= blocks, "у каждого unsafe-блока и unsafe impl нужен комментарий `// SAFETY:` ({blocks} мест, {reasons} комментариев)");
    assert!(!code.contains("pub unsafe fn"), "публичный API должен быть безопасным");
    if code.contains("unsafe fn") {
        assert!(code.contains("# Safety"), "у unsafe fn должен быть раздел `# Safety`");
    }
    assert!(!code.contains("static mut"), "static mut не нужен");
    assert!(!code.contains("transmute"), "transmute не нужен");
}
