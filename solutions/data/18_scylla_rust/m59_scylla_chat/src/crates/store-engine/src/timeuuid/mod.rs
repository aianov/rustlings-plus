use uuid::Uuid;

const GREGORIAN_OFFSET: u64 = 0x01b2_1dd2_1381_4000;

pub fn timeuuid(micros: u64, salt: u64) -> Uuid {
    let ticks = micros
        .saturating_mul(10)
        .saturating_add(GREGORIAN_OFFSET)
        .saturating_add(salt % 10);
    let low = (ticks & 0xffff_ffff) as u32;
    let mid = ((ticks >> 32) & 0xffff) as u16;
    let high = (((ticks >> 48) & 0x0fff) as u16) | 0x1000;
    let mut tail = [0u8; 8];
    let mixed = salt.rotate_left(17) ^ 0x9e37_79b9_7f4a_7c15;
    tail.copy_from_slice(&mixed.to_be_bytes());
    tail[0] = (tail[0] & 0x3f) | 0x80;
    Uuid::from_fields(low, mid, high, &tail)
}

pub fn micros_of(id: &Uuid) -> Option<u64> {
    let (ticks, _) = id.get_timestamp()?.to_gregorian();
    Some(ticks.saturating_sub(GREGORIAN_OFFSET) / 10)
}
