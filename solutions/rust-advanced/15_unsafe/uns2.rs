/// Читает элемент без проверки границ.
///
/// # Safety
///
/// `index` должен быть строго меньше `values.len()`.
pub unsafe fn get_raw(values: &[u32], index: usize) -> u32 {
    // SAFETY: вызывающий гарантирует, что index меньше длины среза.
    unsafe { *values.get_unchecked(index) }
}

pub fn get_or(values: &[u32], index: usize, default: u32) -> u32 {
    if index < values.len() {
        // SAFETY: границу только что проверили.
        unsafe { get_raw(values, index) }
    } else {
        default
    }
}

pub fn sum_first(values: &[u32], count: usize) -> Option<u32> {
    if count > values.len() {
        return None;
    }
    let mut sum = 0_u32;
    for index in 0..count {
        // SAFETY: count не больше длины, значит каждый index меньше длины.
        sum = sum.wrapping_add(unsafe { get_raw(values, index) });
    }
    Some(sum)
}

fn main() {
    println!("{}", get_or(&[1, 2, 3], 5, 0));
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = include_str!("uns2.rs");

    fn head() -> &'static str {
        SRC.split("#[cfg(test)]").next().unwrap_or("")
    }

    #[test]
    fn the_wrapper_checks_bounds() {
        let v = [10, 20, 30];
        assert_eq!(get_or(&v, 0, 99), 10);
        assert_eq!(get_or(&v, 2, 99), 30);
        assert_eq!(get_or(&v, 3, 99), 99, "индекс равен длине: элемента нет");
        assert_eq!(get_or(&v, usize::MAX, 99), 99);
        assert_eq!(get_or(&[], 0, 7), 7);
    }

    #[test]
    fn the_raw_getter_works_for_valid_indices() {
        let v = [5, 6, 7];
        // SAFETY: индексы меньше длины.
        unsafe {
            assert_eq!(get_raw(&v, 0), 5);
            assert_eq!(get_raw(&v, 2), 7);
        }
    }

    #[test]
    fn sums_stop_at_the_slice_end() {
        let v = [1, 2, 3, 4];
        assert_eq!(sum_first(&v, 0), Some(0));
        assert_eq!(sum_first(&v, 3), Some(6));
        assert_eq!(sum_first(&v, 4), Some(10));
        assert_eq!(sum_first(&v, 5), None, "просили больше, чем есть: читать за границей нельзя");
    }

    #[test]
    fn the_contract_is_written_down() {
        let code = head();
        assert!(code.contains("/// # Safety"), "у `unsafe fn` должен быть раздел `# Safety` в документации");
        let blocks = code.matches("unsafe {").count();
        let reasons = code.matches("// SAFETY:").count();
        assert!(blocks > 0, "в задаче должны остаться unsafe-блоки");
        assert!(reasons >= blocks, "у каждого unsafe-блока должен быть комментарий `// SAFETY:` ({blocks} блоков, {reasons} комментариев)");
    }
}
