fn shrink(input: &[u8], still_fails: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    let mut cur = input.to_vec();
    if !still_fails(&cur) {
        return cur;
    }
    let mut chunk = (cur.len() / 2).max(1);
    loop {
        let mut progress = false;
        let mut start = 0;
        while start < cur.len() {
            let end = (start + chunk).min(cur.len());
            let mut candidate = cur[..start].to_vec();
            candidate.extend_from_slice(&cur[end..]);
            if still_fails(&candidate) {
                cur = candidate;
                progress = true;
            } else {
                start += chunk;
            }
        }
        if chunk > 1 {
            chunk = (chunk / 2).max(1);
        } else if !progress {
            break;
        }
    }
    for i in 0..cur.len() {
        if cur[i] != b'a' {
            let mut candidate = cur.clone();
            candidate[i] = b'a';
            if still_fails(&candidate) {
                cur = candidate;
            }
        }
    }
    cur
}

fn main() {
    let small = shrink(b"zzzXzzzYzzz", |d| d.contains(&b'X') && d.contains(&b'Y'));
    println!("{}", String::from_utf8_lossy(&small));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn noise(len: usize, seed: u8) -> Vec<u8> {
        (0..len).map(|i| b'0' + ((i as u8).wrapping_mul(7).wrapping_add(seed)) % 10).collect()
    }

    #[test]
    fn two_needed_bytes_in_a_big_haystack() {
        let mut input = noise(1000, 1);
        input[300] = b'X';
        input[800] = b'Y';
        let still = |d: &[u8]| d.iter().position(|b| *b == b'X').is_some_and(|x| d[x..].contains(&b'Y'));
        assert_eq!(shrink(&input, still), b"XY", "минимум, на котором X ещё стоит перед Y, это ровно «XY»");
    }

    #[test]
    fn result_still_fails_and_is_minimal_by_length() {
        let input = noise(500, 3);
        let result = shrink(&input, |d| d.len() >= 7);
        assert_eq!(result.len(), 7, "короче семи байт предикат не выполняется, длиннее не нужно");
        assert!(result.iter().all(|b| *b == b'a'), "оставшиеся байты надо упростить до 'a': {:?}", String::from_utf8_lossy(&result));
    }

    #[test]
    fn adjacent_needles_are_kept_together() {
        let mut input = noise(600, 9);
        input[250..254].copy_from_slice(b"\r\n\r\n");
        let result = shrink(&input, |d| d.windows(4).any(|w| w == b"\r\n\r\n"));
        assert_eq!(result, b"\r\n\r\n", "упрощение байтов не должно ломать падение: '\\r' на 'a' заменять нельзя");
    }

    #[test]
    fn works_when_single_byte_removal_gets_stuck() {
        let mut input = noise(1024, 5);
        input[420] = b'X';
        let result = shrink(&input, |d| d.len() % 2 == 0 && d.contains(&b'X'));
        assert_eq!(result.len(), 2, "предикат требует чётную длину: убирать надо кусками, один байт за раз тут застревает; получено {} байт", result.len());
        assert!(result.contains(&b'X'));
    }

    #[test]
    fn needs_few_calls_on_huge_inputs() {
        let mut input = noise(200_000, 2);
        input[150_000] = b'X';
        let calls = Cell::new(0u32);
        let result = shrink(&input, |d| {
            calls.set(calls.get() + 1);
            d.contains(&b'X')
        });
        assert_eq!(result, b"X");
        assert!(calls.get() < 2500, "проверок сделано {}: на входе в 200000 байт нужно не больше пары тысяч, значит убирать нужно большими кусками, а не по байту", calls.get());
    }

    #[test]
    fn input_that_does_not_fail_is_returned_unchanged() {
        let input = b"all good".to_vec();
        assert_eq!(shrink(&input, |_| false), input);
        assert_eq!(shrink(b"", |_| true), b"");
    }
}
