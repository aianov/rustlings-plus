// У HTTP/2 есть управление потоком: получатель выдаёт отправителю кредит в байтах отдельно на каждый поток и на всё соединение.
// DATA тратит кредит, WINDOW_UPDATE его пополняет. Если медленный читатель не пополняет кредит, его поток встаёт, а соединение
// и остальные потоки обязаны жить. Отправитель, не следящий за кредитом, нарушает протокол, а следящий слишком усердно
// даёт одному большому потоку съесть всё и оставить соседей ни с чем. TODO: Допиши Sender: poll отдаёт кадры, которые можно
// отправить прямо сейчас (не больше max_frame, в пределах обоих окон, по очереди между потоками), остальное ждёт обновлений.

#![allow(dead_code)]
use std::collections::BTreeMap;

const MAX_WINDOW: i64 = 0x7fff_ffff;

#[derive(Debug, PartialEq)]
enum FlowError {
    ZeroIncrement,
    Overflow,
}

struct Queue {
    window: i64,
    data: Vec<u8>,
}

struct Sender {
    conn_window: i64,
    initial: i64,
    max_frame: usize,
    streams: BTreeMap<u32, Queue>,
}

impl Sender {
    fn new(initial_window: u32, max_frame: usize) -> Sender {
        Sender { conn_window: 65_535, initial: i64::from(initial_window), max_frame, streams: BTreeMap::new() }
    }

    fn open(&mut self, stream: u32) {
        self.streams.insert(stream, Queue { window: self.initial, data: Vec::new() });
    }

    fn push(&mut self, stream: u32, data: &[u8]) {
        if let Some(queue) = self.streams.get_mut(&stream) {
            queue.data.extend_from_slice(data);
        }
    }

    fn pending(&self, stream: u32) -> usize {
        self.streams.get(&stream).map_or(0, |q| q.data.len())
    }

    fn window(&self, stream: u32) -> Option<i64> {
        if stream == 0 { Some(self.conn_window) } else { self.streams.get(&stream).map(|q| q.window) }
    }

    fn window_update(&mut self, stream: u32, increment: u32) -> Result<(), FlowError> {
        let _ = (stream, increment);
        todo!()
    }

    fn set_initial_window(&mut self, new: u32) -> Result<(), FlowError> {
        let _ = new;
        todo!()
    }

    fn poll(&mut self) -> Vec<(u32, Vec<u8>)> {
        todo!()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn total(frames: &[(u32, Vec<u8>)], stream: u32) -> usize {
        frames.iter().filter(|(s, _)| *s == stream).map(|(_, d)| d.len()).sum()
    }

    #[test]
    fn nothing_leaves_beyond_the_window() {
        let mut sender = Sender::new(65_535, 16_384);
        sender.open(1);
        sender.push(1, &vec![7u8; 200_000]);
        let frames = sender.poll();
        assert_eq!(total(&frames, 1), 65_535, "кредит соединения и потока по 65535: больше отправлять нельзя");
        assert!(frames.iter().all(|(_, d)| d.len() <= 16_384), "кадр не больше max_frame");
        assert_eq!(sender.pending(1), 200_000 - 65_535);
        assert_eq!(sender.window(0), Some(0));
        assert_eq!(sender.window(1), Some(0));
        assert!(sender.poll().is_empty(), "кредит кончился, отправлять нечего");
    }

    #[test]
    fn window_update_unblocks_exactly_that_much() {
        let mut sender = Sender::new(65_535, 1000);
        sender.open(1);
        sender.push(1, &vec![1u8; 100_000]);
        sender.poll();
        sender.window_update(1, 5000).unwrap();
        assert!(sender.poll().is_empty(), "пополнен только поток, а кредит соединения по нулям");
        sender.window_update(0, 3000).unwrap();
        let frames = sender.poll();
        assert_eq!(total(&frames, 1), 3000, "уйти может столько, сколько позволяет меньший из двух кредитов");
        sender.window_update(0, 100_000).unwrap();
        let frames = sender.poll();
        assert_eq!(total(&frames, 1), 2000, "теперь ограничивает остаток кредита потока");
    }

    #[test]
    fn a_stalled_stream_does_not_block_the_others() {
        let mut sender = Sender::new(0, 16_384);
        sender.open(1);
        sender.open(3);
        sender.push(1, &[1u8; 5000]);
        sender.push(3, &[3u8; 5000]);
        sender.window_update(3, 2000).unwrap();
        let frames = sender.poll();
        assert_eq!(total(&frames, 3), 2000, "у потока 3 есть кредит, у потока 1 нет: поток 3 должен уйти");
        assert_eq!(total(&frames, 1), 0, "у потока 1 кредита нет");
        assert_eq!(sender.pending(1), 5000);
    }

    #[test]
    fn big_stream_cannot_starve_a_small_one() {
        let mut sender = Sender::new(1_000_000, 4096);
        sender.open(1);
        sender.open(3);
        sender.push(1, &vec![1u8; 500_000]);
        sender.push(3, &[3u8; 1000]);
        let frames = sender.poll();
        assert_eq!(total(&frames, 3), 1000, "малый поток 3 должен целиком уйти в том же проходе: кредит соединения делится по очереди, а не достаётся потоку с меньшим номером");
        assert_eq!(total(&frames, 1), 65_535 - 1000);
    }

    #[test]
    fn zero_and_overflowing_updates_are_errors() {
        let mut sender = Sender::new(65_535, 16_384);
        sender.open(1);
        assert_eq!(sender.window_update(1, 0), Err(FlowError::ZeroIncrement));
        assert_eq!(sender.window_update(0, 0x7fff_ffff), Err(FlowError::Overflow), "65535 + 2^31 - 1 выходит за 2^31 - 1");
        assert_eq!(sender.window(0), Some(65_535), "неудачное пополнение ничего не меняет");
        assert_eq!(sender.window_update(1, 0x7fff_ffff - 65_535), Ok(()));
        assert_eq!(sender.window(1), Some(0x7fff_ffff));
        assert_eq!(sender.window_update(1, 1), Err(FlowError::Overflow));
    }

    #[test]
    fn changing_the_initial_window_shifts_every_stream_and_can_go_negative() {
        let mut sender = Sender::new(65_535, 16_384);
        sender.open(1);
        sender.push(1, &vec![0u8; 60_000]);
        sender.poll();
        assert_eq!(sender.window(1), Some(5535));
        sender.set_initial_window(1000).unwrap();
        assert_eq!(sender.window(1), Some(5535 - 64_535), "изменение начального окна сдвигает окна уже открытых потоков, и они могут стать отрицательными");
        sender.open(3);
        assert_eq!(sender.window(3), Some(1000), "новые потоки стартуют с новым окном");
        sender.push(1, &[1u8; 10]);
        sender.window_update(0, 60_000).unwrap();
        assert!(sender.poll().is_empty(), "поток с отрицательным окном ничего не отправляет");
        sender.window_update(1, 70_000).unwrap();
        let frames = sender.poll();
        assert_eq!(total(&frames, 1), 10);
    }

    #[test]
    fn initial_window_overflow_is_an_error_and_changes_nothing() {
        let mut sender = Sender::new(65_535, 16_384);
        sender.open(1);
        sender.window_update(1, 1_000_000).unwrap();
        assert_eq!(sender.set_initial_window(0x7fff_ffff), Err(FlowError::Overflow));
        assert_eq!(sender.window(1), Some(65_535 + 1_000_000), "ошибка ничего не меняет");
    }
}
