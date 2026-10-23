use feed_builder::{Post, build_feed};
use std::hint::black_box;
use std::time::{Duration, Instant};

fn dataset(count: usize) -> Vec<Post> {
    (0..count)
        .map(|i| Post {
            id: ((i as u64) * 0x9E37_79B1) & 0xFFFF_FFFF,
            author: (i % 40_000) as u32,
            text: format!("пост номер {i} и немного текста про rust, чтобы строка была длиннее сорока символов"),
            likes: (i % 97) as u32,
        })
        .collect()
}

fn main() {
    let posts = dataset(20_000);
    let follows: Vec<u32> = (0..2_000).map(|n| n * 7).collect();
    let blocked: Vec<u32> = (0..200).map(|n| n * 49).collect();
    let mut times: Vec<Duration> = Vec::new();
    for round in 0..8 {
        let start = Instant::now();
        black_box(build_feed(black_box(&posts), black_box(&follows), black_box(&blocked), 20));
        if round >= 2 {
            times.push(start.elapsed());
        }
    }
    times.sort();
    println!("медиана {:?}, худший {:?}", times[times.len() / 2], times[times.len() - 1]);
}
