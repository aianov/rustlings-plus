pub fn shrink(input: &[u8], still_fails: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    let mut current = input.to_vec();
    if !still_fails(&current) {
        return current;
    }
    let mut chunk = (current.len() / 2).max(1);
    loop {
        let mut progress = false;
        let mut start = 0;
        while start < current.len() {
            let end = (start + chunk).min(current.len());
            let mut candidate = current[..start].to_vec();
            candidate.extend_from_slice(&current[end..]);
            if still_fails(&candidate) {
                current = candidate;
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
    for at in 0..current.len() {
        if current[at] != b'a' {
            let mut candidate = current.clone();
            candidate[at] = b'a';
            if still_fails(&candidate) {
                current = candidate;
            }
        }
    }
    current
}
