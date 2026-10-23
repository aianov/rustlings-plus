pub fn checksum(rounds: u64) -> u64 {
    let mut acc = 0xcbf2_9ce4_8422_2325u64;
    for i in 0..rounds {
        acc = (acc ^ i).wrapping_mul(0x0100_0000_01b3);
    }
    acc
}
