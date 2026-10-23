const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0100_0000_01b3;

pub fn checksum(sql: &str) -> String {
    let mut hash = OFFSET;
    for byte in sql.replace("\r\n", "\n").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{hash:016x}")
}
